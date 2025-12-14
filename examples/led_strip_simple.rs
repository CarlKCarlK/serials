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
use serials::led_strip_simple::{SimpleStrip, SimpleStripStatic, colors};
type PioPeriph = embassy_rp::peripherals::PIO0;
type StripStatic = SimpleStripStatic<LEN>;
type Strip = SimpleStrip<'static, PioPeriph, LEN>;

const LEN: usize = 8;
const MAX_CURRENT_MA: u32 = 50;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<!> {
    let peripherals = embassy_rp::init(Default::default());

    // cmk2ai can't we avoid listing the full type twice? It is very long and repetitive.
    static STRIP_STATIC: StripStatic = StripStatic::new_static();
    // cmk2ai So, we don't need a spawner passed in?
    // cmk2ai we avoid IRQ code here by having 3 new functions, correct? I assume generic is impossible? what do we think of one macro instead?
    // cmk test other PIOs.
    let mut strip = led_strip_simple::new_pio0(
        &STRIP_STATIC,
        peripherals.PIO0,
        peripherals.PIN_2,
        MAX_CURRENT_MA,
    );

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut position: isize = 0;
    let mut direction: isize = 1;

    loop {
        update_bounce(&mut strip, position as usize).await?;

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
    // cmk2ai how to other examles avoid the full type here?
    strip: &mut Strip,
    position: usize,
) -> Result<()> {
    assert!(position < LEN);
    let mut pixels = [colors::BLACK; LEN];
    pixels[position] = colors::WHITE;
    strip.update_pixels(&pixels).await?;
    Ok(())
}
