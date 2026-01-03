//! Compile-only verification that the LED strip macros work with PIO1.
//!
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use device_kit::Result;
use device_kit::led_strip::Current;
use device_kit::led_strip::led_strips;
use embassy_executor::Spawner;
use panic_probe as _;

led_strips! {
    pio: PIO1,
    LedStrips {
        gpio16_pio1: { dma: DMA_CH3, pin: PIN_16, len: 48, max_current: Current::Milliamps(100) }
    }
}

/// Verify that led_strips! works with PIO1
async fn test_pio1_strip(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<()> {
    let (_gpio16_pio1_led_strip,) = LedStrips::new(p.PIO1, p.DMA_CH3, p.PIN_16, spawner)?;

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
