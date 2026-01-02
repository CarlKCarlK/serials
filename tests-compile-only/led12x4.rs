//! Compile-only verification for Led12x4 construction using the led2d macros.
//!
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_layout::LedLayout;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led_strip::{Current, colors};
use device_kit::led2d::led2d_from_strip;
use device_kit::pio_split;
use embassy_executor::Spawner;
use panic_probe as _;

const LED12X4_W: usize = 12;
const LED12X4_H: usize = 4;
const LED_LAYOUT_12X4: LedLayout<48, 12, 4> = LedLayout::serpentine_column_major();

define_led_strips! {
    pio: PIO0,
    strips: [
        Gpio3Pio0LedStrip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_3,
            len: 48,
            max_current: Current::Milliamps(500),
            gamma: Gamma::Linear
        }
    ]
}

define_led_strips! {
    pio: PIO1,
    strips: [
        Gpio3Pio1LedStrip {
            sm: 0,
            dma: DMA_CH1,
            pin: PIN_3,
            len: 48,
            max_current: Current::Milliamps(500),
            gamma: Gamma::Linear
        }
    ]
}

led2d_from_strip! {
    pub gpio3_pio0_led2d,
    strip_type: Gpio3Pio0LedStrip,
    width: 12,
    height: 4,
    led_layout: LED_LAYOUT_12X4,
    max_frames: 32,
    font: Font3x4Trim,
}

led2d_from_strip! {
    pub gpio3_pio1_led2d,
    strip_type: Gpio3Pio1LedStrip,
    width: 12,
    height: 4,
    led_layout: LED_LAYOUT_12X4,
    max_frames: 32,
    font: Font3x4Trim,
}

/// Verify Gpio3Pio0LedStrip with write_text
async fn test_led12x4_pio0_write_text(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<()> {
    let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO0);
    let gpio3_pio0_led_strip = Gpio3Pio0LedStrip::new(sm0, p.DMA_CH0, p.PIN_3, spawner)?;

    static LED_12X4_STATIC: Gpio3Pio0Led2dStatic = Gpio3Pio0Led2d::new_static();
    let led_12x4 = Gpio3Pio0Led2d::from_strip(gpio3_pio0_led_strip, spawner)?;

    led_12x4
        .write_text(
            "1234",
            &[colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        )
        .await?;

    Ok(())
}

/// Verify Gpio3Pio1LedStrip constructor
async fn test_led12x4_pio1(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<()> {
    let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO1);
    let gpio3_pio1_led_strip = Gpio3Pio1LedStrip::new(sm0, p.DMA_CH1, p.PIN_3, spawner)?;

    static LED_12X4_STATIC: Gpio3Pio1Led2dStatic = Gpio3Pio1Led2d::new_static();
    let _led_12x4 = Gpio3Pio1Led2d::from_strip(gpio3_pio1_led_strip, spawner)?;

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
