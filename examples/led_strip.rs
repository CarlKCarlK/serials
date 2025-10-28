#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use lib::{define_led_strip, led_strip::Rgb, Result};
use panic_probe as _;

define_led_strip! {
    led_strip0 as LedStrip0 {
        task: led_strip_0_driver,
        pio: PIO1,
        irq: PIO1_IRQ_0,
        sm: { field: sm0, index: 0 },
        dma: DMA_CH1,
        pin: PIN_2,
        len: 8,
        max_current_ma: 500
    }
}


#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    static LED_STRIP_NOTIFIER: LedStrip0::Notifier = LedStrip0::notifier();
    let mut led_strip_0 = LedStrip0::new(
        spawner,
        &LED_STRIP_NOTIFIER,
        peripherals.PIO1,
        peripherals.DMA_CH1,
        peripherals.PIN_2,
    )
    .expect("Failed to start LED strip");

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut led_strip_0, hue)
            .await
            .expect("pattern update failed");

        hue = hue.wrapping_add(3);
        Timer::after_millis(80).await;
    }
}

async fn update_rainbow(strip: &mut LedStrip0::Strip, base: u8) -> Result<()> {
    let mut pixels = [Rgb::default(); LedStrip0::LEN];
    for idx in 0..LedStrip0::LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        pixels[idx] = wheel(offset);
    }
    strip.update_pixels(&pixels).await?;
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
