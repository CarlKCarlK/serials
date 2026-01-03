#![no_std]
#![no_main]

use core::convert::Infallible;

use defmt::info;
use device_kit::Result;
use device_kit::led_strip::{Current, colors, define_led_strip};
use embassy_executor::Spawner;
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

const LEN: usize = 8;
const MAX_CURRENT: Current = Current::Milliamps(50);

define_led_strip! {
    MyLedStrip {
        pio: PIO1,
        pin: PIN_2,
        len: LEN,
        max_current: MAX_CURRENT,
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    // Create strip - no tuple unpacking needed!
    let led_strip = MyLedStrip::new(p.PIO1, p.DMA_CH0, p.PIN_2, spawner)?;

    info!("LED strip initialized with {} LEDs", MyLedStrip::LEN);

    // Create a simple rainbow pattern
    let frame = [
        colors::RED,
        colors::ORANGE,
        colors::YELLOW,
        colors::GREEN,
        colors::CYAN,
        colors::BLUE,
        colors::PURPLE,
        colors::MAGENTA,
    ];

    led_strip.write_frame(frame.into()).await?;
    info!("Rainbow displayed!");

    Timer::after_secs(2).await;

    // Turn off
    led_strip.write_frame([colors::BLACK; LEN].into()).await?;
    info!("LEDs off");

    loop {
        Timer::after_secs(1).await;
    }
}
