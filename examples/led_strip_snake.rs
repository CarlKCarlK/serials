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

// Two WS2812B 4x12 LED matrices (48 pixels each) sharing PIO0
define_led_strips! {
    Gpio2LedStrip {
        pin: PIN_2,
        len: 48,
        max_current: Current::Milliamps(100),
        max_animation_frames: 48,
    },
    Gpio3LedStrip {
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

    // Initialize PIO0 bus (default)
    let (sm0, sm1, _sm2, _sm3) = pio_split!(p.PIO0);

    let gpio2_led_strip = Gpio2LedStrip::new(sm0, p.DMA_CH0, p.PIN_2, spawner)?;
    let gpio3_led_strip = Gpio3LedStrip::new(sm1, p.DMA_CH1, p.PIN_3, spawner)?;

    info!("Dual WS2812B 4x12 Matrix demo starting");
    info!("Using PIO0, two state machines, GPIO2 & GPIO3");
    info!(
        "Max brightness: {}  ({}mA budget each)",
        Gpio2LedStrip::MAX_BRIGHTNESS,
        100
    );

    const SNAKE_LENGTH: usize = 5;
    const SNAKE_COLOR: Rgb = colors::WHITE; // Scaled by max_brightness
    const BACKGROUND: Rgb = colors::BLACK;
    const FRAME_DURATION: embassy_time::Duration = embassy_time::Duration::from_millis(100);

    // Pre-generate all animation frames
    let mut frames = heapless::Vec::<
        (Frame<{ Gpio2LedStrip::LEN }>, embassy_time::Duration),
        { Gpio2LedStrip::LEN },
    >::new();

    for position in 0..Gpio2LedStrip::LEN {
        let mut frame = Frame::<{ Gpio2LedStrip::LEN }>::filled(BACKGROUND);

        // Turn on snake pixels
        for offset in 0..SNAKE_LENGTH {
            let pixel_index = (position + Gpio2LedStrip::LEN - offset) % Gpio2LedStrip::LEN;
            frame[pixel_index] = SNAKE_COLOR;
        }

        frames.push((frame, FRAME_DURATION)).ok();
    }

    info!(
        "Starting snake animation with {} frames on both displays",
        frames.len()
    );

    // Start the animation loop on both strips - they will run forever in the background
    gpio2_led_strip.animate(frames.iter().copied()).await?;
    gpio3_led_strip.animate(frames.into_iter()).await?;

    info!("Snake animations started, entering idle loop");

    // Animations run in background, main task can do other work
    loop {
        Timer::after_secs(10).await;
        info!("Animations still running...");
    }
}
