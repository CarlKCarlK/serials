//! Compile-only verification that multiple led2d devices can coexist in the same file.
//!
//! This demonstrates that the associated constants approach (Led4x12::H, Led8x8::H)
//! prevents namespace collisions when multiple devices are defined.
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_layout::LedLayout;
use device_kit::led_strip::Current;
use device_kit::led_strip::led_strip;
use device_kit::led2d::led2d_from_strip;
use embassy_executor::Spawner;
use embassy_time::Duration;
use panic_probe as _;
use smart_leds::colors;

// Define strips for both devices
led_strip! {
    Gpio3LedStrip {
        pio: PIO0,
        pin: PIN_3,
        len: 48,
        max_current: Current::Milliamps(500),
    }
}

led_strip! {
    Gpio4LedStrip {
        pio: PIO1,
        dma: DMA_CH1,
        pin: PIN_4,
        len: 64,
        max_current: Current::Milliamps(300),
    }
}

const LED_LAYOUT_4X12: LedLayout<48, 12, 4> = LedLayout::serpentine_column_major();
const LED_LAYOUT_8X8: LedLayout<64, 8, 8> = LedLayout::serpentine_column_major();

// First device: 4x12 display
led2d_from_strip! {
    pub led4x12,
    strip_type: Gpio3LedStrip,
    width: 12,
    height: 4,
    led_layout: LED_LAYOUT_4X12,
    max_frames: 32,
    font: Font3x4Trim,
}

// Second device: 8x8 display
led2d_from_strip! {
    pub led8x8,
    strip_type: Gpio4LedStrip,
    width: 8,
    height: 8,
    led_layout: LED_LAYOUT_8X8,
    max_frames: 32,
    font: Font3x4Trim,
}

/// Verify both devices can be constructed and used together
async fn test_multiple_devices(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<()> {
    // Construct first device
    let gpio3_led_strip = Gpio3LedStrip::new(p.PIO0, p.DMA_CH0, p.PIN_3, spawner)?;
    static LED4X12_STATIC: Led4x12Static = Led4x12::new_static();
    let led4x12 = Led4x12::from_strip(gpio3_led_strip, spawner)?;

    // Construct second device
    let gpio4_led_strip = Gpio4LedStrip::new(p.PIO1, p.DMA_CH1, p.PIN_4, spawner)?;
    static LED8X8_STATIC: Led8x8Static = Led8x8::new_static();
    let led8x8 = Led8x8::from_strip(gpio4_led_strip, spawner)?;

    // Verify associated constants don't collide
    // Create frame for 4x12 display
    let mut frame_4x12 = Led4x12::new_frame();
    frame_4x12[0][0] = colors::RED;
    frame_4x12[Led4x12::H - 1][Led4x12::W - 1] = colors::BLUE;
    led4x12.write_frame(frame_4x12).await?;

    // Create frame for 8x8 display (different dimensions)
    let mut frame_8x8 = Led8x8::new_frame();
    frame_8x8[0][0] = colors::GREEN;
    frame_8x8[Led8x8::H - 1][Led8x8::W - 1] = colors::YELLOW;
    led8x8.write_frame(frame_8x8).await?;

    // Verify animations work with both
    let frames_4x12 = [(frame_4x12, Duration::from_millis(100))];
    led4x12.animate(frames_4x12).await?;

    let frames_8x8 = [(frame_8x8, Duration::from_millis(100))];
    led8x8.animate(frames_8x8).await?;

    // Verify N constant is correct for each
    const _N_4X12: usize = Led4x12::N; // Should be 48
    const _N_8X8: usize = Led8x8::N; // Should be 64

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
