#![no_std]
#![no_main]
#![feature(never_type)]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use serials::Result;
use serials::led_strip_simple::{LedStripSimple, LedStripSimpleStatic, colors};
use serials::new_simple_strip;

type StripStatic0 = LedStripSimpleStatic<LEN0>;
type StripStatic1 = LedStripSimpleStatic<LEN1>;

const LEN0: usize = 8;
const LEN1: usize = 48;
const MAX_CURRENT_MA: u32 = 500;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<!> {
    let peripherals = embassy_rp::init(Default::default());

    static STRIP_STATIC_0: StripStatic0 = StripStatic0::new_static();
    let mut strip0 = new_simple_strip!(
        &STRIP_STATIC_0,  // static resources
        PIN_2,            // data pin
        peripherals.PIO0, // PIO block
        MAX_CURRENT_MA    // max current budget (mA)
    );

    static STRIP_STATIC_1: StripStatic1 = StripStatic1::new_static();
    let mut strip1 = new_simple_strip!(
        &STRIP_STATIC_1,  // static resources
        PIN_3,            // data pin
        peripherals.PIO1, // PIO block
        MAX_CURRENT_MA    // max current budget (mA)
    );

    info!("LED strip demo starting (GPIO2 & GPIO3, VSYS power)");

    let mut position0: isize = 0;
    let mut direction0: isize = 1;
    let mut position1: isize = 0;
    let mut direction1: isize = 1;

    loop {
        update_bounce(&mut strip0, position0 as usize).await?;
        update_bounce(&mut strip1, position1 as usize).await?;

        position0 += direction0;
        if position0 <= 0 {
            position0 = 0;
            direction0 = 1;
        } else if position0 as usize >= LEN0 - 1 {
            position0 = (LEN0 - 1) as isize;
            direction0 = -1;
        }

        position1 += direction1;
        if position1 <= 0 {
            position1 = 0;
            direction1 = 1;
        } else if position1 as usize >= LEN1 - 1 {
            position1 = (LEN1 - 1) as isize;
            direction1 = -1;
        }

        Timer::after_millis(500).await;
    }
}

async fn update_bounce<const N: usize, PIO: embassy_rp::pio::Instance>(
    strip: &mut LedStripSimple<'static, PIO, N>,
    position: usize,
) -> Result<()> {
    assert!(position < N);
    let mut pixels = [colors::BLACK; N];
    pixels[position] = colors::WHITE;
    strip.update_pixels(&pixels).await?;
    Ok(())
}
