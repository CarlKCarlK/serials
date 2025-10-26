#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use lib::{LedStrip, LedStripNotifier, Rgb, Result, LED_STRIP_LEN};
use panic_probe as _;

static LED_STRIP_NOTIFIER: LedStripNotifier = LedStrip::notifier();

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    let mut led_strip = LedStrip::new(
        &LED_STRIP_NOTIFIER,
        peripherals.PIO1,
        peripherals.DMA_CH1,
        peripherals.PIN_2,
        spawner,
    )
    .expect("Failed to start LED strip driver");

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut led_strip, hue)
            .await
            .expect("pattern update failed");

        hue = hue.wrapping_add(3);
        Timer::after_millis(80).await;
    }
}

async fn update_rainbow(led_strip: &mut LedStrip, base: u8) -> Result<()> {
    for idx in 0..LED_STRIP_LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        led_strip.update_pixel(idx, wheel(offset)).await?;
    }
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
