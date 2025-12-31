#![no_std]
#![no_main]

// cmk0000 rename to led_strip1
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::{LedStrip, Milliamps, colors, gamma::Gamma, new_led_strip};
use embassy_executor::Spawner;
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    type LedStrip48 = LedStrip<'static, embassy_rp::peripherals::PIO0, 48>;
    const MAX_CURRENT: Milliamps = Milliamps(250);

    let mut led_strip: LedStrip48 = new_led_strip!(
        LED_STRIP,
        48,
        p.PIN_3,
        p.PIO0,
        p.DMA_CH0,
        MAX_CURRENT,
        Gamma::Linear
    )
    .await;

    info!("Setting every other LED to blue on GPIO3");

    let mut frame = [colors::BLACK; LedStrip48::LEN];
    for pixel_index in (0..LedStrip48::LEN).step_by(2) {
        frame[pixel_index] = colors::BLUE;
    }
    led_strip.update_pixels(&frame).await?;

    loop {}
}
