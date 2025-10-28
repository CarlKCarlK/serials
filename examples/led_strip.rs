#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::pio::Pio;
use embassy_rp::{bind_interrupts, pio::InterruptHandler};
use embassy_time::Timer;
use lib::{define_led_strips, define_pio_bus, Rgb, Result};
use panic_probe as _;

bind_interrupts!(struct Pio0Irqs {
    PIO0_IRQ_0 => InterruptHandler<embassy_rp::peripherals::PIO0>;
});

define_pio_bus!(PIO0_BUS, PIO0);

define_led_strips! {
    bus: PIO0_BUS,
    pio: PIO0,
    irqs: Pio0Irqs,
    strips: [
        led_strip0 {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_2,
            len: 8,
            max_current_ma: 480
        }
    ]
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    // Initialize PIO0 bus
    let Pio { common, sm0, .. } = Pio::new(peripherals.PIO0, Pio0Irqs);
    let pio_bus = PIO0_BUS.init_with(|| lib::led_strip::PioBus::new(common));

    static LED_STRIP_NOTIFIER: led_strip0::Notifier = led_strip0::notifier();
    let mut led_strip_0 = led_strip0::new(
        spawner,
        &LED_STRIP_NOTIFIER,
        pio_bus,
        sm0,
        peripherals.DMA_CH0.into(),
        peripherals.PIN_2.into(),
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
    let mut pixels = [Rgb { r: 0, g: 0, b: 0 }; led_strip0::LEN];
    for idx in 0..led_strip0::LEN {
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
