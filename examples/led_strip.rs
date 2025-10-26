#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::pio::InterruptHandler;
use embassy_rp::peripherals::PIO1;
use embassy_time::Timer;
use lib::{define_led_strip, Rgb, Result};
use panic_probe as _;

bind_interrupts!(struct Pio1Irqs {
    PIO1_IRQ_0 => InterruptHandler<PIO1>;
});

define_led_strip! {
    led_strip0 {
        task: led_strip_0_driver,
        pio: PIO1,
        irqs: crate::Pio1Irqs,
        sm: { field: sm0, index: 0 },
        dma: DMA_CH1,
        pin: PIN_2,
        len: 8
    }
}


#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    static LED_STRIP_NOTIFIER: led_strip0::Notifier = led_strip0::notifier();
    let mut led_strip_0 = led_strip0::new(
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

async fn update_rainbow(strip: &mut led_strip0::Strip, base: u8) -> Result<()> {
    for idx in 0..led_strip0::LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        strip.update_pixel(idx, wheel(offset)).await?;
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
