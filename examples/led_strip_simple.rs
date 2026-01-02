#![no_std]
#![no_main]
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led_strip::{LedStrip, Milliamps, colors};
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

const LEN: usize = 8;
const MAX_CURRENT: Milliamps = Milliamps(50);

define_led_strips! {
    pio: PIO1,
    strips: [
        Gpio2LedStrip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_2,
            len: LEN,
            max_current: MAX_CURRENT,
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

    let (pio1_sm0, _pio1_sm1, _pio1_sm2, _pio1_sm3) = pio_split!(p.PIO1);
    let gpio2_led_strip = Gpio2LedStrip::new(pio1_sm0, p.DMA_CH0, p.PIN_2, spawner)?;

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut position: isize = 0;
    let mut direction: isize = 1;

    loop {
        update_bounce(gpio2_led_strip, position as usize).await?;

        position += direction;
        if position <= 0 {
            position = 0;
            direction = 1;
        } else if position as usize >= LEN - 1 {
            position = (LEN - 1) as isize;
            direction = -1;
        }

        Timer::after_millis(500).await;
    }
}

async fn update_bounce(led_strip: &LedStrip<LEN>, position: usize) -> Result<()> {
    assert!(position < LEN);
    let mut pixels = [colors::BLACK; LEN];
    pixels[position] = colors::WHITE;
    led_strip.update_pixels(&pixels).await?;
    Ok(())
}
