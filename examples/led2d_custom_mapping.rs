#![no_std]
#![no_main]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::init;
use embassy_time::{Duration, Timer};
use panic_probe as _;
use serials::Result;
use serials::led2d::{Frame, led2d_device_simple};

// Example with a custom arbitrary mapping for a 2x3 display (6 LEDs total)
// This demonstrates a simple row-major ordering: rows go left-to-right
led2d_device_simple! {
    pub led2x3,
    rows: 2,
    cols: 3,
    pio: PIO0,
    mapping: arbitrary([
        0, 1, 2,  // Row 0: LEDs 0, 1, 2
        3, 4, 5,  // Row 1: LEDs 3, 4, 5
    ]),
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("LED 2D Custom Mapping Example (2x3 display with row-major mapping)");
    let p = init(Default::default());

    static LED2X3_STATIC: Led2x3Static = Led2x3::new_static();
    let led2x3 = Led2x3::new(&LED2X3_STATIC, p.PIO0, p.PIN_3, Milliamps(100), spawner).await?;

    loop {
        info!("Lighting corners");
        demo_corners(&led2x3).await?;
        Timer::after_secs(2).await;

        info!("Chasing animation");
        demo_chase(&led2x3).await?;
    }
}

async fn demo_corners(led2x3: &Led2x3) -> Result<()> {
    let black = colors::BLACK;
    let mut frame = [black; N];

    // Light the four corners
    frame[led2x3.xy_to_index(0, 0)] = colors::RED; // Top-left
    frame[led2x3.xy_to_index(COLS - 1, 0)] = colors::GREEN; // Top-right
    frame[led2x3.xy_to_index(0, ROWS - 1)] = colors::BLUE; // Bottom-left
    frame[led2x3.xy_to_index(COLS - 1, ROWS - 1)] = colors::YELLOW; // Bottom-right

    led2x3.write_frame(frame).await?;
    Ok(())
}

async fn demo_chase(led2x3: &Led2x3) -> Result<()> {
    let black = colors::BLACK;

    // Create frames for each LED position
    let mut frames: [Frame<N>; N] = [Frame::new([black; N], Duration::from_millis(200)); N];

    for led_index in 0..N {
        let mut frame = [black; N];
        frame[led_index] = colors::CYAN;
        frames[led_index] = Frame::new(frame, Duration::from_millis(200));
    }

    led2x3.animate(&frames).await
}
