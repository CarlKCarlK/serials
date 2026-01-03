//! Compile-only verification that the LED strip macros work with PIO0.
//!
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use device_kit::Result;
use device_kit::led_strip::Current;
use device_kit::led_strip::define_led_strips;
use embassy_executor::Spawner;
use panic_probe as _;

define_led_strips! {
    LedStrips {
        gpio2_pio0: { pin: PIN_2, len: 8, max_current: Current::Milliamps(50) }
    }
}

/// Verify that define_led_strips! works with PIO0
async fn test_pio0_strip(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<()> {
    let (_gpio2_pio0_led_strip,) = LedStrips::new(p.PIO0, p.DMA_CH0, p.PIN_2, spawner)?;

    Ok(())
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // This main function exists only to satisfy the compiler.
    // The actual verification happens at compile time via the function above.
}

#[cfg(not(any(target_arch = "arm", target_arch = "riscv32", target_arch = "riscv64")))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
