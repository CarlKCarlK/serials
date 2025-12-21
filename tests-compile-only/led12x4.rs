//! Compile-only verification for Led12x4 construction using the led2d macros.
//!
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![cfg(not(feature = "host"))]
#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use defmt_rtt as _;
use embassy_executor::Spawner;
use panic_probe as _;
use serials::led_strip_simple::{Milliamps, colors};
use serials::led2d::{Led2dFont, led2d_device, led2d_device_simple};

led2d_device_simple! {
    pub led12x4_pio0,
    rows: 4,
    cols: 12,
    pio: PIO0,
    mapping: serpentine_column_major,
    max_frames: 32,
    font: Led2dFont::Font3x4Trim,
}

led2d_device_simple! {
    pub led12x4_pio1,
    rows: 4,
    cols: 12,
    pio: PIO1,
    mapping: serpentine_column_major,
    max_frames: 32,
    font: Led2dFont::Font3x4Trim,
}

const LED12X4_ROWS: usize = 4;
const LED12X4_COLS: usize = 12;
const LED12X4_N: usize = LED12X4_ROWS * LED12X4_COLS;
const LED12X4_MAX_FRAMES: usize = 32;
const LED12X4_MAPPING: [u16; LED12X4_N] =
    serials::led2d::serpentine_column_major_mapping::<LED12X4_N, LED12X4_ROWS, LED12X4_COLS>();
type LedFrame = serials::led2d::Frame<LED12X4_ROWS, LED12X4_COLS>;
static LED12X4_STRIP_STATIC: serials::led_strip::LedStripStatic<LED12X4_N> =
    serials::led_strip::LedStrip::new_static();

led2d_device! {
    pub struct Led12x4StripResources,
    task: pub led12x4_strip_task,
    strip: serials::led_strip::LedStrip<LED12X4_N>,
    leds: LED12X4_N,
    mapping: &LED12X4_MAPPING,
    cols: LED12X4_COLS,
    max_frames: LED12X4_MAX_FRAMES
}

async fn write_text_frame(
    led: &serials::led2d::Led2d<'static, LED12X4_N, LED12X4_MAX_FRAMES>,
) -> serials::Result<()> {
    let frame = LedFrame::new();
    led.write_frame(frame).await
}

/// Verify Led12x4Pio0 with write_text
async fn test_led12x4_pio0_write_text(
    p: embassy_rp::Peripherals,
    spawner: Spawner,
) -> serials::Result<()> {
    static LED_12X4_STATIC: Led12x4Pio0Static = Led12x4Pio0::new_static();
    let led_12x4 =
        Led12x4Pio0::new(&LED_12X4_STATIC, p.PIO0, p.PIN_3, Milliamps(500), spawner).await?;

    led_12x4
        .write_text(
            "1234",
            &[colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        )
        .await?;

    Ok(())
}

/// Verify Led12x4Pio1 constructor
async fn test_led12x4_pio1(p: embassy_rp::Peripherals, spawner: Spawner) -> serials::Result<()> {
    static LED_12X4_STATIC: Led12x4Pio1Static = Led12x4Pio1::new_static();
    let _led_12x4 =
        Led12x4Pio1::new(&LED_12X4_STATIC, p.PIO1, p.PIN_3, Milliamps(500), spawner).await?;

    Ok(())
}

/// Verify Led2d with a custom strip type (multi-strip driver)
async fn test_led12x4_from_multi(
    _p: embassy_rp::Peripherals,
    spawner: Spawner,
) -> serials::Result<()> {
    static LED12X4_RESOURCES: Led12x4StripResources = Led12x4StripResources::new_static();
    let led_strip = serials::led_strip::LedStrip::new(&LED12X4_STRIP_STATIC)?;
    let led12x4 = LED12X4_RESOURCES.new(led_strip, spawner)?;
    write_text_frame(&led12x4).await?;

    Ok(())
}

/// Verify Led2d with a manually constructed LedStripSimple
async fn test_led12x4_from_simple(
    p: embassy_rp::Peripherals,
    spawner: Spawner,
) -> serials::Result<()> {
    let _ = p;
    static LED12X4_RESOURCES: Led12x4StripResources = Led12x4StripResources::new_static();
    let led_strip = serials::led_strip::LedStrip::new(&LED12X4_STRIP_STATIC)?;
    let led12x4 = LED12X4_RESOURCES.new(led_strip, spawner)?;
    write_text_frame(&led12x4).await?;

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
