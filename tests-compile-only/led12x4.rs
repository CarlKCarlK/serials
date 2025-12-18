//! Compile-only verification for Led12x4 construction.
//!
//! These functions verify that Led12x4 can be constructed from different LED strip types
//! without requiring actual hardware. This file is designed to type-check during compilation,
//! ensuring the API works correctly.
//!
//! Run via: `cargo check-all` (xtask compiles this for thumbv6m-none-eabi)

#![no_std]
#![no_main]
#![allow(dead_code, reason = "Compile-time verification only")]

use defmt_rtt as _;
use embassy_executor::Spawner;
use panic_probe as _;
use serials::led_strip::define_led_strips;
use serials::led_strip_simple::{LedStripSimpleStatic, Milliamps, new_simple_strip};
use serials::led12x4::{Led12x4, Led12x4Static, Led12x4Strip, new_led12x4};

/// Verify Led12x4 from PIO0 with write_text
async fn test_led12x4_pio0_write_text(
    p: embassy_rp::Peripherals,
    spawner: Spawner,
) -> serials::Result<()> {
    use serials::led_strip_simple::colors;

    static LED_12X4_STATIC: Led12x4Static = Led12x4Static::new_static();
    let led_12x4 = new_led12x4!(&LED_12X4_STATIC, PIN_3, p.PIO0, Milliamps(500), spawner).await?;

    led_12x4
        .write_text(
            ['1', '2', '3', '4'],
            [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        )
        .await?;

    Ok(())
}

/// Verify Led12x4 from PIO1
async fn test_led12x4_pio1(p: embassy_rp::Peripherals, spawner: Spawner) -> serials::Result<()> {
    static LED_12X4_STATIC: Led12x4Static = Led12x4Static::new_static();
    let _led_12x4 = new_led12x4!(&LED_12X4_STATIC, PIN_3, p.PIO1, Milliamps(500), spawner).await?;

    Ok(())
}

/// Verify Led12x4::from with LedStripSimple
async fn test_led12x4_from_simple(
    p: embassy_rp::Peripherals,
    spawner: Spawner,
) -> serials::Result<()> {
    static LED_STRIP_SIMPLE_STATIC: LedStripSimpleStatic<48> = LedStripSimpleStatic::new_static();
    let led_strip_simple =
        new_simple_strip!(&LED_STRIP_SIMPLE_STATIC, PIN_3, p.PIO1, Milliamps(500)).await;

    static LED_12X4_STATIC: Led12x4Static = Led12x4Static::new_static();
    let _led_12x4 = Led12x4::from(&LED_12X4_STATIC, Led12x4Strip::SimplePio1(led_strip_simple), spawner)?;

    Ok(())
}

/// Verify Led12x4::from with multi-strip driver
async fn test_led12x4_from_multi(
    p: embassy_rp::Peripherals,
    spawner: Spawner,
) -> serials::Result<()> {
    define_led_strips! {
        pio: PIO1,
        strips: [
            led12x4_strip {
                sm: 0,
                dma: DMA_CH0,
                pin: PIN_3,
                len: 48,
                max_current: Milliamps(500)
            }
        ]
    }

    let (pio_bus, sm0, _sm1, _sm2, _sm3) = pio1_split(p.PIO1);

    static LED12X4_STRIP_STATIC: led12x4_strip::Static = led12x4_strip::new_static();
    let led12x4_strip = led12x4_strip::new(
        spawner,
        &LED12X4_STRIP_STATIC,
        pio_bus,
        sm0,
        p.DMA_CH0.into(),
        p.PIN_3.into(),
    )?;

    static LED_12X4_STATIC: Led12x4Static = Led12x4Static::new_static();
    let _led_12x4 = Led12x4::from(&LED_12X4_STATIC, Led12x4Strip::Multi(led12x4_strip), spawner)?;

    Ok(())
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // This main function exists only to satisfy the compiler.
    // The actual verification happens at compile time via the functions above.
}
