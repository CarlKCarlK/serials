#![no_std]
#![no_main]
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::led_strip;
use device_kit::led_strip::{Current, Frame, colors};
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

const LEN: usize = 8;
const MAX_CURRENT: Current = Current::Milliamps(50);

led_strip! {
    Gpio0LedStrip {
        pio: PIO1,
        pin: PIN_0,
        len: LEN,
        max_current: MAX_CURRENT,
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    let gpio0_led_strip = Gpio0LedStrip::new(p.PIO1, p.DMA_CH0, p.PIN_0, spawner)?;

    info!("LED strip demo starting (GPIO0 data, VSYS power)");

    let mut position: isize = 0;
    let mut direction: isize = 1;

    loop {
        update_bounce(&gpio0_led_strip, position as usize).await?;

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

async fn update_bounce(led_strip: &Gpio0LedStrip, position: usize) -> Result<()> {
    assert!(position < LEN);
    let mut frame = Frame::<LEN>::new();
    frame[position] = colors::WHITE;
    led_strip.write_frame(frame).await?;
    Ok(())
}
