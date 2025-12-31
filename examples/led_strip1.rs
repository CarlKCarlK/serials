#![no_std]
#![no_main]

// cmk0000 rename to led_strip1 (may no longer apply)
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::{Milliamps, colors, gamma::Gamma, new_led_strip};
use embassy_executor::Spawner;
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    let mut led_strip = new_led_strip!(
        LED_STRIP,
        48,
        p.PIN_3,
        p.PIO0,
        p.DMA_CH0,
        Milliamps(250),
        Gamma::Linear
    )
    .await;

    info!("Setting every other LED to blue on GPIO3");

    let mut frame = [colors::BLACK; 48];
    for pixel_index in (0..frame.len()).step_by(2) {
        frame[pixel_index] = colors::BLUE;
    }
    led_strip.update_pixels(&frame).await?;

    loop {}
}
