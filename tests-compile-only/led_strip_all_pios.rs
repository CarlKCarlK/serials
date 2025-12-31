#![no_std]
#![no_main]

use panic_probe as _;

/// Compile-only test to verify `new_led_strip!` macro works with all PIO blocks.
/// This prevents type mismatches between the macro call and type aliases.
#[allow(dead_code)]
async fn test_all_pios(p: embassy_rp::Peripherals) {
    use device_kit::led_strip::{LedStrip, Milliamps, gamma::Gamma, new_led_strip};

    // Test PIO0
    type LedStrip48Pio0 = LedStrip<'static, embassy_rp::peripherals::PIO0, 48>;
    let _led_strip_pio0: LedStrip48Pio0 = new_led_strip!(
        LED_STRIP_PIO0,
        48,
        p.PIN_3,
        p.PIO0,
        p.DMA_CH0,
        Milliamps(250),
        Gamma::Linear
    )
    .await;

    // Test PIO1
    type LedStrip48Pio1 = LedStrip<'static, embassy_rp::peripherals::PIO1, 48>;
    let _led_strip_pio1: LedStrip48Pio1 = new_led_strip!(
        LED_STRIP_PIO1,
        48,
        p.PIN_4,
        p.PIO1,
        p.DMA_CH1,
        Milliamps(250),
        Gamma::Linear
    )
    .await;

    // Test PIO2 (Pico 2 only)
    #[cfg(feature = "pico2")]
    {
        type LedStrip48Pio2 = LedStrip<'static, embassy_rp::peripherals::PIO2, 48>;
        let _led_strip_pio2: LedStrip48Pio2 = new_led_strip!(
            LED_STRIP_PIO2,
            48,
            p.PIN_5,
            p.PIO2,
            p.DMA_CH2,
            Milliamps(250),
            Gamma::Linear
        )
        .await;
    }
}
