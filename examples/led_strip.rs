#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::led_strip;
use device_kit::led_strip::{Current, Frame, Rgb};
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
    match inner_main(spawner).await {
        Ok(_) => unreachable!(),
        Err(e) => panic!("Fatal error: {:?}", e),
    }
}

async fn inner_main(spawner: Spawner) -> Result<()> {
    let p = embassy_rp::init(Default::default());

    let gpio0_led_strip = Gpio0LedStrip::new(p.PIO0, p.DMA_CH0, p.PIN_0, spawner)?;

    info!("LED strip demo starting (GPIO0 data, VSYS power)");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(gpio2_led_strip, hue).await?;

        hue = hue.wrapping_add(3);
        Timer::after_millis(80).await;
    }
}

async fn update_rainbow(led_strip: &Gpio2LedStrip, base: u8) -> Result<()> {
    let mut frame = Frame::<{ Gpio2LedStrip::LEN }>::new();
    for idx in 0..Gpio2LedStrip::LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        frame[idx] = wheel(offset);
    }
    led_strip.write_frame(frame).await?;
    Ok(())
}

fn wheel(pos: u8) -> Rgb {
    let pos = 255 - pos;
    if pos < 85 {
        rgb(255 - pos * 3, 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        rgb(0, pos * 3, 255 - pos * 3)
    } else {
        let pos = pos - 170;
        rgb(pos * 3, 255 - pos * 3, 0)
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}
