#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use defmt_rtt as _;

use device_kit::Result;
use device_kit::led_strip::Current;
use device_kit::led_strip::led_strips;
use embassy_executor::Spawner;

const MAX_CURRENT: Current = Current::Milliamps(250);

led_strips! {
    pio: PIO0,
    LedStripsPio0 {
        pio0: { pin: PIN_3, len: 48, max_current: MAX_CURRENT }
    }
}

led_strips! {
    pio: PIO1,
    LedStripsPio1 {
        pio1: { dma: DMA_CH1, pin: PIN_4, len: 48, max_current: MAX_CURRENT }
    }
}

#[cfg(feature = "pico2")]
led_strips! {
    pio: PIO2,
    LedStripsPio2 {
        pio2: { dma: DMA_CH2, pin: PIN_5, len: 48, max_current: MAX_CURRENT }
    }
}

/// Compile-only test to verify `led_strips!` works with all PIO blocks.
/// This prevents type mismatches between generated strip types and PIO splits.
#[allow(dead_code)]
async fn test_all_pios(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<()> {
    let (_pio0_led_strip_48,) = LedStripsPio0::new(p.PIO0, p.DMA_CH0, p.PIN_3, spawner)?;
    let (_pio1_led_strip_48,) = LedStripsPio1::new(p.PIO1, p.DMA_CH1, p.PIN_4, spawner)?;

    // Test PIO2 (Pico 2 only)
    #[cfg(feature = "pico2")]
    {
        let (_pio2_led_strip_48,) = LedStripsPio2::new(p.PIO2, p.DMA_CH2, p.PIN_5, spawner)?;
    }

    Ok(())
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // This main function exists only to satisfy the compiler.
}

// panic_probe provides a panic handler for host, but we need one for embedded
#[cfg(target_arch = "arm")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
