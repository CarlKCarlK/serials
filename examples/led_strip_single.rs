#![no_std]
#![no_main]

use core::convert::Infallible;

use defmt::info;
use device_kit::Result;
use device_kit::led_strip::{Current, colors, led_strip};
use embassy_executor::Spawner;
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

led_strip! {
    LedStrip {
        pin: PIN_0,
        len: 8,
        max_current: Current::Milliamps(50),
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    // Create strip - no tuple unpacking needed!
    let led_strip = LedStrip::new(p.PIO0, p.DMA_CH0, p.PIN_0, spawner)?;

    info!("LED strip initialized with {} LEDs", LedStrip::LEN);

    // Create a simple rainbow pattern
    let frame = [
        colors::RED,
        colors::ORANGE,
        colors::YELLOW,
        colors::GREEN,
        colors::CYAN,
        colors::BLUE,
        colors::PURPLE,
        colors::MAGENTA,
    ];

    led_strip.write_frame(frame.into()).await?;
    info!("Rainbow displayed!");

    Timer::after_secs(2).await;

    // Turn off
    led_strip
        .write_frame([colors::BLACK; LedStrip::LEN].into())
        .await?;
    info!("LEDs off");

    loop {
        Timer::after_secs(1).await;
    }
}
