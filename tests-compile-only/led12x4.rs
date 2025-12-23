//! Compile-only verification for Led12x4 construction using the led2d macros.
//!
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use defmt_rtt as _;
use device_kit::led_strip::led_strip_shared::define_led_strips;
use device_kit::led_strip::{Milliamps, colors};
use device_kit::led2d::led2d_from_strip;
use device_kit::pio_split;
use embassy_executor::Spawner;
use panic_probe as _;

const LED12X4_ROWS: usize = 4;
const LED12X4_COLS: usize = 12;

define_led_strips! {
    pio: PIO0,
    strips: [
        led12x4_pio0_strip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_3,
            len: 48,
            max_current: Milliamps(500)
        }
    ]
}

define_led_strips! {
    pio: PIO1,
    strips: [
        led12x4_pio1_strip {
            sm: 0,
            dma: DMA_CH1,
            pin: PIN_3,
            len: 48,
            max_current: Milliamps(500)
        }
    ]
}

led2d_from_strip! {
    pub led12x4_pio0,
    strip_module: led12x4_pio0_strip,
    rows: 4,
    cols: 12,
    mapping: serpentine_column_major,
    max_frames: 32,
    font: Font3x4Trim,
}

led2d_from_strip! {
    pub led12x4_pio1,
    strip_module: led12x4_pio1_strip,
    rows: 4,
    cols: 12,
    mapping: serpentine_column_major,
    max_frames: 32,
    font: Font3x4Trim,
}

/// Verify Led12x4Pio0 with write_text
async fn test_led12x4_pio0_write_text(
    p: embassy_rp::Peripherals,
    spawner: Spawner,
) -> device_kit::Result<()> {
    let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO0);
    let led12x4_pio0_strip = led12x4_pio0_strip::new(sm0, p.DMA_CH0, p.PIN_3, spawner)?;

    static LED_12X4_STATIC: Led12x4Pio0Static = Led12x4Pio0::new_static();
    let led_12x4 = Led12x4Pio0::from_strip(led12x4_pio0_strip, spawner)?;

    led_12x4
        .write_text(
            "1234",
            &[colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        )
        .await?;

    Ok(())
}

/// Verify Led12x4Pio1 constructor
async fn test_led12x4_pio1(p: embassy_rp::Peripherals, spawner: Spawner) -> device_kit::Result<()> {
    let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO1);
    let led12x4_pio1_strip = led12x4_pio1_strip::new(sm0, p.DMA_CH1, p.PIN_3, spawner)?;

    static LED_12X4_STATIC: Led12x4Pio1Static = Led12x4Pio1::new_static();
    let _led_12x4 = Led12x4Pio1::from_strip(led12x4_pio1_strip, spawner)?;

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
