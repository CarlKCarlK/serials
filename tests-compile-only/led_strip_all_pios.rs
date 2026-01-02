#![no_std]
#![no_main]

use panic_probe as _;

use device_kit::led_strip::Milliamps;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::gamma::Gamma;
use device_kit::Result;

const MAX_CURRENT: Milliamps = Milliamps(250);

define_led_strips! {
    pio: PIO0,
    strips: [
        Pio0LedStrip48 {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_3,
            len: 48,
            max_current: MAX_CURRENT,
            gamma: Gamma::Linear
        }
    ]
}

define_led_strips! {
    pio: PIO1,
    strips: [
        Pio1LedStrip48 {
            sm: 0,
            dma: DMA_CH1,
            pin: PIN_4,
            len: 48,
            max_current: MAX_CURRENT,
            gamma: Gamma::Linear
        }
    ]
}

#[cfg(feature = "pico2")]
define_led_strips! {
    pio: PIO2,
    strips: [
        Pio2LedStrip48 {
            sm: 0,
            dma: DMA_CH2,
            pin: PIN_5,
            len: 48,
            max_current: MAX_CURRENT,
            gamma: Gamma::Linear
        }
    ]
}

/// Compile-only test to verify `define_led_strips!` works with all PIO blocks.
/// This prevents type mismatches between generated strip types and PIO splits.
#[allow(dead_code)]
async fn test_all_pios(
    p: embassy_rp::Peripherals,
    spawner: embassy_executor::Spawner,
) -> Result<()> {
    use device_kit::pio_split;

    let (pio0_sm0, _pio0_sm1, _pio0_sm2, _pio0_sm3) = pio_split!(p.PIO0);
    let (pio1_sm0, _pio1_sm1, _pio1_sm2, _pio1_sm3) = pio_split!(p.PIO1);

    let _pio0_led_strip_48 = Pio0LedStrip48::new(pio0_sm0, p.DMA_CH0, p.PIN_3, spawner)?;
    let _pio1_led_strip_48 = Pio1LedStrip48::new(pio1_sm0, p.DMA_CH1, p.PIN_4, spawner)?;

    // Test PIO2 (Pico 2 only)
    #[cfg(feature = "pico2")]
    {
        let (pio2_sm0, _pio2_sm1, _pio2_sm2, _pio2_sm3) = pio_split!(p.PIO2);
        let _pio2_led_strip_48 =
            Pio2LedStrip48::new(pio2_sm0, p.DMA_CH2, p.PIN_5, spawner)?;
    }

    Ok(())
}
