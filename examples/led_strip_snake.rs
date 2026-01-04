#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::led_strips;
use device_kit::led_strip::{Frame, Rgb, colors};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use panic_probe as _;

// Two WS2812B 4x12 LED matrices (48 pixels each) sharing PIO0
led_strips! {
    LedStrips {
        gpio3: { pin: PIN_3, len: 48, max_frames: 48 },
        gpio4: { pin: PIN_4, len: 48, max_frames: 48 }
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

    let (gpio3_led_strip, gpio4_led_strip) =
        LedStrips::new(p.PIO0, p.DMA_CH0, p.PIN_3, p.DMA_CH1, p.PIN_4, spawner)?;

    info!("Dual WS2812B 4x12 Matrix demo starting");
    info!("Using PIO0, two state machines, GPIO3 & GPIO4");
    info!(
        "Max brightness: {}  ({}mA budget each)",
        Gpio3LedStrip::MAX_BRIGHTNESS,
        100
    );

    const FRAME_DURATION: Duration = Duration::from_millis(300);
    const BRIGHT: Rgb = colors::WHITE;
    const GAP: Rgb = colors::BLACK;
    const GAP_SPACING: usize = 4;
    const FRAME_COUNT: usize = GAP_SPACING;

    let mut frames =
        heapless::Vec::<(Frame<{ Gpio3LedStrip::LEN }>, Duration), FRAME_COUNT>::new();

    for frame_offset in 0..FRAME_COUNT {
        let mut frame = Frame::<{ Gpio3LedStrip::LEN }>::filled(BRIGHT);
        for pixel_index in 0..Gpio3LedStrip::LEN {
            if (pixel_index + frame_offset) % GAP_SPACING == 0 {
                frame[pixel_index] = GAP;
            }
        }
        frames.push((frame, FRAME_DURATION)).ok();
    }

    info!(
        "Starting Broadway-style animation with {} frames per strip",
        frames.len()
    );

    // Start the animation loop on both strips - they will run forever in the background
    gpio3_led_strip.animate(frames.clone()).await?;
    gpio4_led_strip.animate(frames).await?;

    info!("Snake animations started, entering idle loop");

    // Animations run in background, main task can do other work
    loop {
        Timer::after_secs(10).await;
        info!("Animations still running...");
    }
}
