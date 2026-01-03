#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Current, Frame, Rgb, colors};
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

// WS2812B 4x12 LED matrix (48 pixels)
// Uses PIO1, State Machine 0, DMA_CH1, GPIO3
// Max 500mA current budget (safe for USB 2.0)
define_led_strips! {
    pio: PIO1,
    Gpio16LedStrip {
        dma: DMA_CH1,
        pin: PIN_3,
        len: 48,
        max_current: Current::Milliamps(100),
        max_animation_frames: 48,
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    match inner_main(spawner).await {
        Ok(_) => unreachable!(),
        Err(e) => panic!("Fatal error: {:?}", e),
    }
}

async fn inner_main(spawner: Spawner) -> Result<()> {
    let p = embassy_rp::init(Default::default());

    // Initialize PIO1 bus
    let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO1);

    let gpio16_led_strip = Gpio16LedStrip::new(sm0, p.DMA_CH1, p.PIN_3, spawner)?;

    info!("WS2812B 4x12 Matrix demo starting");
    info!("Using PIO1, DMA_CH1, GPIO3");
    info!(
        "Max brightness: {}  ({}mA budget)",
        Gpio16LedStrip::MAX_BRIGHTNESS,
        500
    );
    info!("Wiring: Red->VSYS(pin39), White->GND(pin38), Green->GPIO3(pin5)");

    const SNAKE_LENGTH: usize = 5;
    const SNAKE_COLOR: Rgb = colors::WHITE; // Scaled by max_brightness
    const BACKGROUND: Rgb = colors::BLACK;
    const FRAME_DURATION: embassy_time::Duration = embassy_time::Duration::from_millis(100);

    // Pre-generate all animation frames
    let mut frames = heapless::Vec::<
        (Frame<{ Gpio16LedStrip::LEN }>, embassy_time::Duration),
        { Gpio16LedStrip::LEN },
    >::new();

    for position in 0..Gpio16LedStrip::LEN {
        let mut frame = Frame::<{ Gpio16LedStrip::LEN }>::filled(BACKGROUND);

        // Turn on snake pixels
        for offset in 0..SNAKE_LENGTH {
            let pixel_index = (position + Gpio16LedStrip::LEN - offset) % Gpio16LedStrip::LEN;
            frame[pixel_index] = SNAKE_COLOR;
        }

        frames.push((frame, FRAME_DURATION)).ok();
    }

    info!("Starting snake animation with {} frames", frames.len());

    // Start the animation loop - it will run forever in the background
    gpio16_led_strip.animate(frames.into_iter()).await?;

    info!("Snake animation loop started, entering idle loop");

    // Animation runs in background, main task can do other work
    loop {
        Timer::after_secs(10).await;
        info!("Animation still running...");
    }
}
