#![no_std]
#![no_main]

// cmk0000 rename to led_strip1 (may no longer apply)
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Current, colors, gamma::Gamma};
use device_kit::pio_split;
use embassy_executor::Spawner;
use panic_probe as _;

define_led_strips! {
    pio: PIO0,
    strips: [
        Gpio3LedStrip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_3,
            len: 48,
            max_current: Current::Milliamps(250),
            gamma: Gamma::Linear
        }
    ]
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    let (pio0_sm0, _pio0_sm1, _pio0_sm2, _pio0_sm3) = pio_split!(p.PIO0);
    let gpio3_led_strip = Gpio3LedStrip::new(pio0_sm0, p.DMA_CH0, p.PIN_3, spawner)?;

    info!("Setting every other LED to blue on GPIO3");

    let mut frame = [colors::BLACK; 48];
    for pixel_index in (0..frame.len()).step_by(2) {
        frame[pixel_index] = colors::BLUE;
    }
    gpio3_led_strip.update_pixels(&frame).await?;

    loop {}
}
