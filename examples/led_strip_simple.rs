#![no_std]
#![no_main]
#![feature(never_type)]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use serials::Result;
use serials::led_strip_simple;

use serials::led_strip_simple::colors;
// cmk2ai If every PIO has a dedicated IRQ, why must IRQ be exposed to the user?
// Answer: `led_strip_simple::init_pio0/init_pio1` bind the PIO IRQs internally, so the example never touches IRQ wiring.

type PioPeriph = embassy_rp::peripherals::PIO0;

// cmk2ai why must the user calculated this? Compare with src/led_strip.rs.
// Answer: Users still choose length and current budget; `max_brightness` turns that into a safe cap like the macro API does.
const LEN: usize = 8;
const MAX_CURRENT_MA: u32 = 50;
const MAX_BRIGHTNESS: u8 = led_strip_simple::max_brightness(LEN, MAX_CURRENT_MA);

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<!> {
    let peripherals = embassy_rp::init(Default::default());

    // Choose PIO and data pin here
    // cmk2ai test that really works with other PIOs (and pins)
    // Answer: swap to `init_pio1(peripherals.PIO1)` and a different GPIO to verify; the helper binds the right IRQ internally.
    let (bus, sm) = led_strip_simple::init_pio0(peripherals.PIO0);
    let pin = peripherals.PIN_2;

    // cmk2ai why do we need a task. If it is because of example is so complex we shoul simplify the example, to perhaps a white dot bounding back and forth.
    // Answer: Driver runs inline here; no background task is spawned.
    let mut driver = led_strip_simple::new_driver_grb::<PioPeriph, 0, LEN>(bus, sm, pin);

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut position: isize = 0;
    let mut direction: isize = 1;

    loop {
        update_bounce(&mut driver, position as usize).await?;

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

async fn update_bounce(
    driver: &mut serials::led_strip_simple::PioWs2812Cpu<'static, PioPeriph, 0, LEN>,
    position: usize,
) -> Result<()> {
    assert!(position < LEN);
    let mut pixels = [colors::BLACK; LEN];
    pixels[position] = colors::WHITE;
    led_strip_simple::apply_max_brightness(&mut pixels, MAX_BRIGHTNESS);
    driver.write(&pixels).await;
    Ok(())
}
