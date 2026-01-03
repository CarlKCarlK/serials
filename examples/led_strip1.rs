#![no_std]
#![no_main]

// cmk0000 rename to led_strip1 (may no longer apply)
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::led_strip;
use device_kit::led_strip::{Current, Frame, colors};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use panic_probe as _;

// cmk000 is this defaulting to dma0 going to be confusing with wifi?
led_strip! {
    LedStrip {
        pin: PIN_3,
        len: 48,
        max_current: Current::Milliamps(250),
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

// cmk000 much cleaner with new()!
// cmk000 is the spawner input in the standard position?
async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    let led_strip = LedStrip::new(p.PIO0, p.DMA_CH0, p.PIN_3, spawner)?;

    info!("Setting every other LED to blue on GPIO3");

    let mut frame = Frame::<48>::new();
    for pixel_index in (0..frame.len()).step_by(2) {
        frame[pixel_index] = colors::BLUE;
    }
    led_strip.write_frame(frame).await?;

    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}
