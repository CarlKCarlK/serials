#![no_std]
#![no_main]
#![feature(never_type)]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::pio::StateMachine;
use embassy_time::Timer;
use panic_probe as _;
use serials::Result;
use serials::led_strip_simple::{self, LedStrip, LedStripCommands, LedStripStatic, PioBus};
use smart_leds::colors;
use static_cell::StaticCell;

bind_interrupts!(struct Pio0Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});

type PioPeriph = embassy_rp::peripherals::PIO0;
type Pin = embassy_rp::peripherals::PIN_2;

const LEN: usize = 8;
const MAX_CURRENT_MA: u32 = 50;
const MAX_BRIGHTNESS: u8 = led_strip_simple::max_brightness(LEN, MAX_CURRENT_MA);

fn init_pio_bus(
    pio: embassy_rp::Peri<'static, PioPeriph>,
) -> (
    &'static PioBus<'static, PioPeriph>,
    StateMachine<'static, PioPeriph, 0>,
) {
    static PIO0_BUS: StaticCell<PioBus<'static, PioPeriph>> = StaticCell::new();

    let embassy_rp::pio::Pio { common, sm0, .. } = embassy_rp::pio::Pio::new(pio, Pio0Irqs);
    let bus = PIO0_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

#[embassy_executor::task]
async fn led_strip0_driver(
    bus: &'static PioBus<'static, PioPeriph>,
    sm: StateMachine<'static, PioPeriph, 0>,
    pin: embassy_rp::Peri<'static, Pin>,
    commands: &'static LedStripCommands<LEN>,
) -> ! {
    led_strip_simple::run_driver_grb::<PioPeriph, 0, LEN>(bus, sm, pin, commands, MAX_BRIGHTNESS)
        .await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    let peripherals = embassy_rp::init(Default::default());

    // Choose PIO and data pin here
    let pio = peripherals.PIO0;
    let pin = peripherals.PIN_2;

    static LED_STRIP_STATIC: LedStripStatic<LEN> = LedStrip::new_static();
    let mut led_strip_simple_0 = LedStrip::new(&LED_STRIP_STATIC)?;

    let (bus, sm) = init_pio_bus(pio);
    let token = led_strip0_driver(bus, sm, pin, LED_STRIP_STATIC.commands())
        .map_err(serials::Error::TaskSpawn)?;
    spawner.spawn(token);

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut led_strip_simple_0, hue).await?;

        hue = hue.wrapping_add(3);
        Timer::after_millis(80).await;
    }
}

async fn update_rainbow(strip: &mut LedStrip<LEN>, base: u8) -> Result<()> {
    let mut pixels = [colors::BLACK; LEN];
    for idx in 0..LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        pixels[idx] = led_strip_simple::wheel(offset);
    }
    strip.update_pixels(&pixels).await?;
    Ok(())
}
