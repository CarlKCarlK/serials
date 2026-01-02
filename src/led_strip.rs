//! A device abstraction for WS2812-style LED strips.
//!
//! See [`LedStrip`] for the simple single-strip driver, or use [`define_led_strips_shared!`] for managing multiple strips on one PIO.

pub mod gamma;

include!("led_strip/led_strip_shared.rs");
// See [`LedStrip`] for the main usage example and [`LedStripShared`] / [`define_led_strips_shared!`] for multi-strip setups on one PIO.

use embassy_rp::pio::{Pio, PioPin, StateMachine as EmbassyStateMachine};
use embassy_rp::pio_programs::ws2812::Grb;
#[doc(inline)]
pub use smart_leds::colors;
use static_cell::StaticCell;

/// Used by [`new_led_strip!`] and [`define_led_strips_shared!`] to budget current for LED strips.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Milliamps(pub u16);

impl Milliamps {
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }
}

// PIO interrupt bindings are defined in lib.rs and imported via crate::pio_irqs
#[cfg(feature = "pico2")]
use crate::pio_irqs::Pio2Irqs;
use crate::pio_irqs::{Pio0Irqs, Pio1Irqs};

static PIO0_BUS: StaticCell<PioBus<'static, embassy_rp::peripherals::PIO0>> = StaticCell::new();
static PIO1_BUS: StaticCell<PioBus<'static, embassy_rp::peripherals::PIO1>> = StaticCell::new();
#[cfg(feature = "pico2")]
static PIO2_BUS: StaticCell<PioBus<'static, embassy_rp::peripherals::PIO2>> = StaticCell::new();

/// Initializes PIO0 with its IRQ bound and returns the shared bus plus SM0.
pub(crate) fn init_pio0(
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
) -> (
    &'static PioBus<'static, embassy_rp::peripherals::PIO0>,
    EmbassyStateMachine<'static, embassy_rp::peripherals::PIO0, 0>,
) {
    let Pio { common, sm0, .. } = Pio::new(pio, Pio0Irqs);
    let bus = PIO0_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

/// Initializes PIO1 with its IRQ bound and returns the shared bus plus SM0.
pub(crate) fn init_pio1(
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO1>,
) -> (
    &'static PioBus<'static, embassy_rp::peripherals::PIO1>,
    EmbassyStateMachine<'static, embassy_rp::peripherals::PIO1, 0>,
) {
    let Pio { common, sm0, .. } = Pio::new(pio, Pio1Irqs);
    let bus = PIO1_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

#[cfg(feature = "pico2")]
/// Initializes PIO2 with its IRQ bound and returns the shared bus plus SM0.
pub(crate) fn init_pio2(
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO2>,
) -> (
    &'static PioBus<'static, embassy_rp::peripherals::PIO2>,
    EmbassyStateMachine<'static, embassy_rp::peripherals::PIO2, 0>,
) {
    let Pio { common, sm0, .. } = Pio::new(pio, Pio2Irqs);
    let bus = PIO2_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

/// Builds a GRB-order DMA driver without spawning a task; caller drives frames directly.
pub(crate) fn new_driver_grb<PIO, const N: usize, Dma>(
    bus: &'static PioBus<'static, PIO>,
    sm: EmbassyStateMachine<'static, PIO, 0>,
    dma: embassy_rp::Peri<'static, Dma>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
) -> PioWs2812<'static, PIO, 0, N, Grb>
where
    PIO: Instance,
    Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
{
    let program = bus.get_program();
    bus.with_common(|common| PioWs2812::<PIO, 0, N, _>::new(common, sm, dma, pin, program))
}

fn max_brightness_for<const N: usize>(max_current: Milliamps) -> u8 {
    assert!(N > 0, "strip must contain at least one LED");
    assert!(max_current.0 > 0, "max_current must be positive");

    let led_count = u64::try_from(N).expect("strip length fits in u64");
    let numerator = u64::from(max_current.as_u32()) * 255;
    let denominator = led_count * 60; // 60mA per LED at full white.
    let brightness = numerator / denominator;

    if brightness >= 255 {
        255
    } else {
        brightness as u8
    }
}

/// Applies a combined gamma correction and brightness cap to an entire frame in place.
fn apply_combo_table<const N: usize>(frame: &mut [Rgb; N], combo_table: &[u8; 256]) {
    for color in frame.iter_mut() {
        *color = Rgb::new(
            combo_table[usize::from(color.r)],
            combo_table[usize::from(color.g)],
            combo_table[usize::from(color.b)],
        );
    }
}

/// Static resources backing a [`LedStrip`] instance.
///
/// See [`LedStrip`] for the usage example.
#[doc(hidden)] // Must be pub for method signatures, but users interact via macro
pub struct LedStripStatic<const N: usize> {
    _priv: (),
}

impl<const N: usize> LedStripStatic<N> {
    /// Number of LEDs in the strip.
    pub const LEN: usize = N;

    #[must_use]
    pub const fn new() -> Self {
        Self { _priv: () }
    }
}

/// Standalone device abstraction for a single WS2812-style LED strip created by [`new_led_strip!`] (one strip per PIO).
///
/// Each Pico contains two (Pico 1) or three (Pico 2) PIO units.
/// This driver consumes one PIO (SM0) and one DMA channel. The more complex [`LedStripShared`] can drive up to four strips per PIO using [`define_led_strips_shared!`].
///
/// # Example
/// ```no_run
/// # #![no_std]
/// # use panic_probe as _;
/// # fn main() {}
/// use device_kit::led_strip::{
///     LedStrip,
///     LedStripStatic,
///     Milliamps,
///     colors,
///     gamma::Gamma,
///     new_led_strip,
/// };
/// use device_kit::Result;
///
/// async fn example(p: embassy_rp::Peripherals) -> Result<()> {
///     let mut led_strip = new_led_strip!(
///         LED_STRIP,        // static name
///         8,                // LED count
///         p.PIN_2,          // data pin
///         p.PIO0,           // PIO block (SM0)
///         p.DMA_CH0,        // DMA channel
///         Milliamps(50),    // max current budget (mA)
///         Gamma::Linear     // gamma correction (Linear or Gamma2_2)
///     ).await;
///
///     let mut frame = [colors::BLACK; 8];
///     frame[0] = colors::WHITE;
///     led_strip.update_pixels(&frame).await?;
///     Ok(())
/// }
/// ```
#[deprecated(note = "Use LedStripShared via define_led_strips_shared! instead.")]
pub struct LedStrip<'d, PIO: Instance, const N: usize> {
    driver: PioWs2812<'d, PIO, 0, N, Grb>,
    combo_table: [u8; 256],
}

#[allow(deprecated)]
impl<'d, PIO: Instance, const N: usize> LedStrip<'d, PIO, N> {
    /// Number of LEDs in this strip.
    pub const LEN: usize = N;

    /// Construct a new inline strip driver from shared bus/state machine and pin.
    pub(crate) fn new<Dma>(
        strip_static: &'static LedStripStatic<N>,
        bus: &'static PioBus<'static, PIO>,
        sm: EmbassyStateMachine<'static, PIO, 0>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        gamma: gamma::Gamma,
        max_brightness: u8,
    ) -> Self
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
    {
        let _ = strip_static; // marker to match Device/Static pattern
        let driver = new_driver_grb::<PIO, N, _>(bus, sm, dma, pin);
        let combo_table = gamma::generate_combo_table(gamma, max_brightness);
        Self {
            driver,
            combo_table,
        }
    }

    /// Update all pixels at once.
    ///
    /// See [`LedStrip`] for the usage example.
    pub async fn update_pixels(&mut self, pixels: &[Rgb; N]) -> Result<()> {
        let mut frame = *pixels;
        apply_combo_table(&mut frame, &self.combo_table);
        self.driver.write(&frame).await;
        Ok(())
    }
}

#[allow(deprecated)]
impl<const N: usize> LedStrip<'static, embassy_rp::peripherals::PIO0, N> {
    /// Builds a `LedStrip` on PIO0/SM0.
    ///
    /// Each Pico contains two (Pico 1) or three (Pico 2) PIO units; this driver requires one PIO (SM0) and one DMA channel. The more complex [LedStripShared] can drive up to four strips per PIO.
    ///
    /// See [`LedStrip`] for the usage example.
    pub(crate) async fn new_pio0<Dma>(
        strip_static: &'static LedStripStatic<N>,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
        gamma: gamma::Gamma,
    ) -> Self
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
    {
        let max_brightness = max_brightness_for::<N>(max_current);
        let (bus, sm) = init_pio0(pio);
        let mut led_strip = LedStrip::new(strip_static, bus, sm, dma, pin, gamma, max_brightness);
        // Initialize with blank frame to ensure LEDs are ready
        let blank = [Rgb::new(0, 0, 0); N];
        led_strip.update_pixels(&blank).await.ok();
        // WS2812 requires minimum 50μs reset period to latch data
        embassy_time::Timer::after_micros(80).await;
        led_strip
    }
}

#[allow(deprecated)]
impl<const N: usize> LedStrip<'static, embassy_rp::peripherals::PIO1, N> {
    /// Builds a `LedStrip` on PIO1/SM0.
    ///
    /// Each Pico contains two (Pico 1) or three (Pico 2) PIO units; this driver requires one PIO (SM0) and one DMA channel. The more complex [LedStripShared] can drive up to four strips per PIO.
    ///
    /// See [`LedStrip`] for the usage example.
    pub(crate) async fn new_pio1<Dma>(
        strip_static: &'static LedStripStatic<N>,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO1>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
        gamma: gamma::Gamma,
    ) -> Self
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
    {
        let max_brightness = max_brightness_for::<N>(max_current);
        let (bus, sm) = init_pio1(pio);
        let mut led_strip = LedStrip::new(strip_static, bus, sm, dma, pin, gamma, max_brightness);
        // Initialize with blank frame to ensure LEDs are ready
        let blank = [Rgb::new(0, 0, 0); N];
        led_strip.update_pixels(&blank).await.ok();
        // WS2812 requires minimum 50μs reset period to latch data
        embassy_time::Timer::after_micros(80).await;
        led_strip
    }
}

#[cfg(feature = "pico2")]
#[allow(deprecated)]
impl<const N: usize> LedStrip<'static, embassy_rp::peripherals::PIO2, N> {
    /// Builds a `LedStrip` on PIO2/SM0.
    ///
    /// Each Pico contains two (Pico 1) or three (Pico 2) PIO units; this driver requires one PIO (SM0) and one DMA channel. The more complex [LedStripShared] can drive up to four strips per PIO.
    ///
    /// See [`LedStrip`] for the usage example.
    pub(crate) async fn new_pio2<Dma>(
        strip_static: &'static LedStripStatic<N>,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO2>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
        gamma: gamma::Gamma,
    ) -> Self
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
    {
        let max_brightness = max_brightness_for::<N>(max_current);
        let (bus, sm) = init_pio2(pio);
        let mut led_strip = LedStrip::new(strip_static, bus, sm, dma, pin, gamma, max_brightness);
        // Initialize with blank frame to ensure LEDs are ready
        let blank = [Rgb::new(0, 0, 0); N];
        led_strip.update_pixels(&blank).await.ok();
        // WS2812 requires minimum 50μs reset period to latch data
        embassy_time::Timer::after_micros(80).await;
        led_strip
    }
}

/// Helper trait for dispatching to the correct `new_pioX()` constructor.
/// Implementation detail of the [`new_led_strip!`] macro.
#[doc(hidden)]
#[allow(deprecated)]
pub trait LedStripNew<const N: usize> {
    async fn new_from_pio<Dma>(
        strip_static: &'static LedStripStatic<N>,
        pio: embassy_rp::Peri<'static, Self>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
        gamma: gamma::Gamma,
    ) -> LedStrip<'static, Self, N>
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
        Self: embassy_rp::pio::Instance;
}

#[allow(deprecated)]
impl<const N: usize> LedStripNew<N> for embassy_rp::peripherals::PIO0 {
    async fn new_from_pio<Dma>(
        strip_static: &'static LedStripStatic<N>,
        pio: embassy_rp::Peri<'static, Self>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
        gamma: gamma::Gamma,
    ) -> LedStrip<'static, Self, N>
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
    {
        LedStrip::new_pio0(strip_static, pio, dma, pin, max_current, gamma).await
    }
}

#[allow(deprecated)]
impl<const N: usize> LedStripNew<N> for embassy_rp::peripherals::PIO1 {
    async fn new_from_pio<Dma>(
        strip_static: &'static LedStripStatic<N>,
        pio: embassy_rp::Peri<'static, Self>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
        gamma: gamma::Gamma,
    ) -> LedStrip<'static, Self, N>
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
    {
        LedStrip::new_pio1(strip_static, pio, dma, pin, max_current, gamma).await
    }
}

#[cfg(feature = "pico2")]
#[allow(deprecated)]
impl<const N: usize> LedStripNew<N> for embassy_rp::peripherals::PIO2 {
    async fn new_from_pio<Dma>(
        strip_static: &'static LedStripStatic<N>,
        pio: embassy_rp::Peri<'static, Self>,
        dma: embassy_rp::Peri<'static, Dma>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
        gamma: gamma::Gamma,
    ) -> LedStrip<'static, Self, N>
    where
        Dma: embassy_rp::dma::Channel + embassy_rp::PeripheralType,
    {
        LedStrip::new_pio2(strip_static, pio, dma, pin, max_current, gamma).await
    }
}

#[doc(hidden)]
#[macro_export]
/// Macro wrapper that routes to `new_pio0`/`new_pio1`/`new_pio2` and hides static creation.
/// See the usage example on [`LedStrip`].
macro_rules! new_led_strip {
    // Main API: name, len, pin, pio, dma, max_current, gamma
    (
        $name:ident,
        $len:literal,
        $pin:expr,
        $pio:expr,
        $dma:expr,
        $max_current:expr,
        $gamma:expr
    ) => {{
        use $crate::led_strip::LedStripNew as _;
        static $name: $crate::led_strip::LedStripStatic<$len> =
            $crate::led_strip::LedStripStatic::new();
        <_ as $crate::led_strip::LedStripNew<$len>>::new_from_pio(
            &$name,
            $pio,
            $dma,
            $pin,
            $max_current,
            $gamma,
        )
    }};
}

/// Macro wrapper that routes to `new_pio0`/`new_pio1`/`new_pio2` and fails fast if PIO2 is used on Pico 1.
/// See the usage example on [`LedStrip`].
#[doc(inline)]
pub use new_led_strip;
