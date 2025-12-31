#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_layout::LedLayout;
use device_kit::led_strip::Milliamps;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led2d::led2d;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use panic_probe as _;
use smart_leds::colors;

// Build a 24x4 display by concatenating two 12x4 serpentine panels horizontally.
const LED_LAYOUT_12X4: LedLayout<48, 12, 4> = LedLayout::<48, 12, 4>::serpentine_column_major();
const LED_LAYOUT_24X4: LedLayout<96, 24, 4> =
    LED_LAYOUT_12X4.concat_h::<48, 96, 12, 24>(LED_LAYOUT_12X4);

led2d! {
    pub led24x4_concat,
    pio: PIO1,
    pin: PIN_4,
    dma: DMA_CH1,
    width: 24,
    height: 4,
    led_layout: LED_LAYOUT_24X4,
    max_current: Milliamps(1000),
    gamma: Gamma::Gamma2_2,
    max_frames: 8,
    font: Font3x4Trim,
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<()> {
    info!("Starting 24x4 concat-h demo");
    let p = embassy_rp::init(Default::default());

    let led24x4_concat = Led24x4Concat::new(p.PIO1, p.DMA_CH1, p.PIN_4, spawner)?;

    let mut frame = Led24x4Concat::new_frame();
    led24x4_concat.write_text_to_frame("HELLO MOM", &[colors::CYAN, colors::WHITE], &mut frame)?;

    loop {
        led24x4_concat.write_frame(frame).await?;
        Timer::after(Duration::from_millis(750)).await;
    }
}
