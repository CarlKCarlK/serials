//! Compile-only verification for Led2d with custom mapping.
//!
//! This verifies that led2d_from_strip! macro works with arbitrary custom mappings.
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip_simple::Milliamps;
use device_kit::led2d;
use embassy_executor::Spawner;
use embassy_time::Duration;
use panic_probe as _;
use smart_leds::colors;

// Example with a custom arbitrary mapping for a 2x3 display (6 LEDs total)
// This demonstrates a simple row-major ordering: rows go left-to-right
led2d! {
    pub led2x3,
    pio: PIO0,
    pin: PIN_3,
    dma: DMA_CH0,
    rows: 2,
    cols: 3,
    mapping: arbitrary([
        0, 1, 2,  // Row 0: LEDs 0, 1, 2
        3, 4, 5,  // Row 1: LEDs 3, 4, 5
    ]),
    max_current: Milliamps(100),
    max_frames: 6,
    font: Font3x4Trim,
}

/// Verify Led2x3 construction with custom mapping
async fn test_led2x3_custom_mapping(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<()> {
    let led2x3 = Led2x3::new(p.PIO0, p.DMA_CH0, p.PIN_3, spawner)?;

    // Verify write_frame works
    let mut frame = Led2x3::new_frame();
    frame[0][0] = colors::RED;
    frame[0][Led2x3::COLS - 1] = colors::GREEN;
    frame[Led2x3::ROWS - 1][0] = colors::BLUE;
    frame[Led2x3::ROWS - 1][Led2x3::COLS - 1] = colors::YELLOW;
    led2x3.write_frame(frame).await?;

    // Verify animate works
    let mut frames = heapless::Vec::<_, { Led2x3::MAX_FRAMES }>::new();
    for row_index in 0..Led2x3::ROWS {
        for column_index in 0..Led2x3::COLS {
            let mut frame = Led2x3::new_frame();
            frame[row_index][column_index] = colors::CYAN;
            frames.push((frame, Duration::from_millis(200))).ok();
        }
    }
    led2x3.animate(&frames).await?;

    Ok(())
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // This main function exists only to satisfy the compiler.
    // The actual verification happens at compile time via the functions above.
}

#[cfg(not(any(target_arch = "arm", target_arch = "riscv32", target_arch = "riscv64")))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
