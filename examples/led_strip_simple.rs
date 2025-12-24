#![no_std]
#![no_main]
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use device_kit::Result;
use device_kit::led_strip::{
    LedStrip, LedStripStatic, Milliamps, colors, new_strip,
};
type PioPeriph = embassy_rp::peripherals::PIO1;
type StripStatic = LedStripStatic<LEN>;
type Strip = LedStrip<'static, PioPeriph, LEN>;

const LEN: usize = 8;
const MAX_CURRENT: Milliamps = Milliamps(50);

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    static STRIP_STATIC: StripStatic = StripStatic::new_static();
    let mut simple_strip = new_strip!(
        &STRIP_STATIC, // static resources
        PIN_2,         // data pin
        p.PIO1,        // PIO block
        MAX_CURRENT    // max current budget (mA)
    )
    .await;

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut position: isize = 0;
    let mut direction: isize = 1;

    loop {
        update_bounce(&mut simple_strip, position as usize).await?;

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
