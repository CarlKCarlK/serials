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

led_strip! {
    Gpio0LedStrip {
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

    let gpio0_led_strip = Gpio0LedStrip::new(p.PIO0, p.DMA_CH0, p.PIN_0, spawner)?;

    info!("LED strip demo starting (GPIO0 data, VSYS power)");

    let mut position: isize = 0;
    let mut direction: isize = 1;

    loop {
        update_bounce(&gpio0_led_strip, position as usize).await?;

        position += direction;
        if position <= 0 {
            position = 0;
            direction = 1;
        } else if position as usize >= Gpio0LedStrip::LEN - 1 {
            position = (Gpio0LedStrip::LEN - 1) as isize;
            direction = -1;
        }

        Timer::after_millis(500).await;
    }
}

async fn update_bounce(led_strip: &Gpio0LedStrip, position: usize) -> Result<()> {
    assert!(position < Gpio0LedStrip::LEN);
    let mut frame = Frame::<{ Gpio0LedStrip::LEN }>::new();
    frame[position] = colors::WHITE;
    led_strip.write_frame(frame).await?;
    Ok(())
}
