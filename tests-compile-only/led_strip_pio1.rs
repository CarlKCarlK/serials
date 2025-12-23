//! Compile-only verification that led_strip works with PIO1.
//!
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use device_kit::led_strip::define_led_strips;
use device_kit::led_strip_simple::Milliamps;
use device_kit::pio_split;
use embassy_executor::Spawner;
use panic_probe as _;

define_led_strips! {
    pio: PIO1,
    strips: [
        test_strip {
            sm: 1,
            dma: DMA_CH3,
            pin: PIN_16,
            len: 48,
            max_current: Milliamps(100)
        }
    ]
}

/// Verify that define_led_strips! works with PIO1
async fn test_pio1_strip(p: embassy_rp::Peripherals, spawner: Spawner) -> device_kit::Result<()> {
    let (_sm0, sm1, _sm2, _sm3) = pio_split!(p.PIO1);

    static TEST_STRIP_STATIC: test_strip::Static = test_strip::new_static();
    let _strip = test_strip::new(&TEST_STRIP_STATIC, sm1, p.DMA_CH3, p.PIN_16, spawner)?;

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
