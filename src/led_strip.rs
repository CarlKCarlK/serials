//! A device abstraction for WS2812-style LED strips.
//! See [`LedStripSimple`] for the main usage example and [led_strip_shared](crate::led_strip::led_strip_shared) for the fuller driver if you need more than a couple of strips.

pub mod led_strip_shared;

use core::cell::RefCell;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::pio::program::{Assembler, JmpCondition, OutDestination, SetDestination, SideSet};
use embassy_rp::pio::{
    Common, Config, FifoJoin, Instance, LoadedProgram, PioPin, ShiftConfig, ShiftDirection,
    StateMachine,
};
use embassy_rp::pio_programs::ws2812::{Grb, RgbColorOrder};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::once_lock::OnceLock;
use embassy_time::{Duration, Timer};
use fixed::types::U24F8;
use smart_leds::RGB8;
/// RGB color constants.
pub use smart_leds::colors;
use static_cell::StaticCell;

use crate::Result;

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

/// Current budget for LED strips, specified in milliamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Milliamps(pub u16);

impl Milliamps {
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }
}

const T1: u8 = 2;
const T2: u8 = 5;
const T3: u8 = 3;
const CYCLES_PER_BIT: u32 = (T1 + T2 + T3) as u32;
const RESET_DELAY_US: u64 = 55;

// PIO interrupt bindings are defined in lib.rs and imported via crate::pio_irqs
#[cfg(feature = "pico2")]
use crate::pio_irqs::Pio2Irqs;
use crate::pio_irqs::{Pio0Irqs, Pio1Irqs};

static PIO0_BUS: StaticCell<PioBus<'static, embassy_rp::peripherals::PIO0>> = StaticCell::new();
static PIO1_BUS: StaticCell<PioBus<'static, embassy_rp::peripherals::PIO1>> = StaticCell::new();
#[cfg(feature = "pico2")]
static PIO2_BUS: StaticCell<PioBus<'static, embassy_rp::peripherals::PIO2>> = StaticCell::new();

/// Shared PIO bus that loads and reuses the WS2812 program.
pub(crate) struct PioBus<'d, PIO: Instance> {
    common: Mutex<CriticalSectionRawMutex, RefCell<Common<'d, PIO>>>,
    program: OnceLock<LoadedProgram<'d, PIO>>,
}

impl<'d, PIO: Instance> PioBus<'d, PIO> {
    /// Creates a bus around a PIO common resource.
    pub fn new(common: Common<'d, PIO>) -> Self {
        Self {
            common: Mutex::new(RefCell::new(common)),
            program: OnceLock::new(),
        }
    }

    /// Returns the loaded WS2812 program, initializing it once.
    pub fn program(&'static self) -> &'static LoadedProgram<'d, PIO> {
        self.program.get_or_init(|| {
            self.common.lock(|cell| {
                let mut common = cell.borrow_mut();
                load_ws2812_program(&mut *common)
            })
        })
    }

    /// Grants temporary mutable access to the shared common resource.
    pub fn with_common<R>(&self, f: impl FnOnce(&mut Common<'d, PIO>) -> R) -> R {
        self.common.lock(|cell| {
            let mut common = cell.borrow_mut();
            f(&mut *common)
        })
    }
}

/// Initializes PIO0 with its IRQ bound and returns the shared bus plus SM0.
pub(crate) fn init_pio0(
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
) -> (
    &'static PioBus<'static, embassy_rp::peripherals::PIO0>,
    StateMachine<'static, embassy_rp::peripherals::PIO0, 0>,
) {
    let embassy_rp::pio::Pio { common, sm0, .. } = embassy_rp::pio::Pio::new(pio, Pio0Irqs);
    let bus = PIO0_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

/// Initializes PIO1 with its IRQ bound and returns the shared bus plus SM0.
pub(crate) fn init_pio1(
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO1>,
) -> (
    &'static PioBus<'static, embassy_rp::peripherals::PIO1>,
    StateMachine<'static, embassy_rp::peripherals::PIO1, 0>,
) {
    let embassy_rp::pio::Pio { common, sm0, .. } = embassy_rp::pio::Pio::new(pio, Pio1Irqs);
    let bus = PIO1_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

#[cfg(feature = "pico2")]
/// Initializes PIO2 with its IRQ bound and returns the shared bus plus SM0.
pub(crate) fn init_pio2(
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO2>,
) -> (
    &'static PioBus<'static, embassy_rp::peripherals::PIO2>,
    StateMachine<'static, embassy_rp::peripherals::PIO2, 0>,
) {
    let embassy_rp::pio::Pio { common, sm0, .. } = embassy_rp::pio::Pio::new(pio, Pio2Irqs);
    let bus = PIO2_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

fn load_ws2812_program<'d, PIO: Instance>(common: &mut Common<'d, PIO>) -> LoadedProgram<'d, PIO> {
    let side_set = SideSet::new(false, 1, false);
    let mut assembler: Assembler<32> = Assembler::new_with_side_set(side_set);

    let mut wrap_target = assembler.label();
    let mut wrap_source = assembler.label();
    let mut do_zero = assembler.label();
    assembler.set_with_side_set(SetDestination::PINDIRS, 1, 0);
    assembler.bind(&mut wrap_target);
    assembler.out_with_delay_and_side_set(OutDestination::X, 1, T3 - 1, 0);
    assembler.jmp_with_delay_and_side_set(JmpCondition::XIsZero, &mut do_zero, T1 - 1, 1);
    assembler.jmp_with_delay_and_side_set(JmpCondition::Always, &mut wrap_target, T2 - 1, 1);
    assembler.bind(&mut do_zero);
    assembler.nop_with_delay_and_side_set(T2 - 1, 0);
    assembler.bind(&mut wrap_source);

    let program = assembler.assemble_with_wrap(wrap_source, wrap_target);
    common.load_program(&program)
}

/// CPU-fed WS2812 driver for a single state machine.
pub(crate) struct PioWs2812Cpu<'d, P: Instance, const S: usize, const N: usize, ORDER = Grb>
where
    ORDER: RgbColorOrder,
{
    sm: StateMachine<'d, P, S>,
    _order: core::marker::PhantomData<ORDER>,
}

impl<'d, P: Instance, const S: usize, const N: usize, ORDER> PioWs2812Cpu<'d, P, S, N, ORDER>
where
    ORDER: RgbColorOrder,
{
    /// Configures the state machine and prepares it for writes.
    pub fn new(
        pio: &mut Common<'d, P>,
        sm: StateMachine<'d, P, S>,
        pin: embassy_rp::Peri<'d, impl PioPin>,
        program: &LoadedProgram<'d, P>,
    ) -> Self {
        let mut cfg = Config::default();

        let out_pin = pio.make_pio_pin(pin);
        cfg.set_out_pins(&[&out_pin]);
        cfg.set_set_pins(&[&out_pin]);
        cfg.use_program(program, &[&out_pin]);

        let clock_freq = U24F8::from_num(clk_sys_freq() / 1000);
        let ws2812_freq = U24F8::from_num(800);
        let bit_freq = ws2812_freq * CYCLES_PER_BIT;
        cfg.clock_divider = clock_freq / bit_freq;

        cfg.fifo_join = FifoJoin::TxOnly;
        cfg.shift_out = ShiftConfig {
            auto_fill: true,
            threshold: 24,
            direction: ShiftDirection::Left,
        };

        let mut sm = sm;
        sm.set_config(&cfg);
        sm.set_enable(true);

        Self {
            sm,
            _order: core::marker::PhantomData,
        }
    }

    /// Writes a full frame to the TX FIFO.
    pub async fn write(&mut self, colors: &[Rgb; N]) {
        let mut words = [0u32; N];
        for (idx, color) in colors.iter().enumerate() {
            words[idx] = ORDER::pack(*color);
        }

        let tx = self.sm.tx();
        for word in words {
            tx.wait_push(word).await;
        }

        Timer::after(Duration::from_micros(RESET_DELAY_US)).await;
    }
}

/// Builds a GRB-order driver without spawning a task; caller drives frames directly.
pub(crate) fn new_driver_grb<PIO, const S: usize, const N: usize>(
    bus: &'static PioBus<'static, PIO>,
    sm: StateMachine<'static, PIO, S>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
) -> PioWs2812Cpu<'static, PIO, S, N, Grb>
where
    PIO: Instance,
{
    let program = bus.program();
    bus.with_common(|common| PioWs2812Cpu::<PIO, S, N, Grb>::new(common, sm, pin, program))
}

#[inline]
fn scale_brightness(value: u8, brightness: u8) -> u8 {
    ((u16::from(value) * u16::from(brightness)) / 255) as u8
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

/// Applies a brightness cap to an entire frame in place.
fn apply_max_brightness<const N: usize>(frame: &mut [Rgb; N], max_brightness: u8) {
    for color in frame.iter_mut() {
        *color = Rgb::new(
            scale_brightness(color.r, max_brightness),
            scale_brightness(color.g, max_brightness),
            scale_brightness(color.b, max_brightness),
        );
    }
}

/// Static resources backing a [`LedStripSimple`] instance.
///
/// See [`LedStripSimple`] for the usage example.
pub struct LedStripSimpleStatic<const N: usize> {
    _priv: (),
}

impl<const N: usize> LedStripSimpleStatic<N> {
    /// Number of LEDs in the strip.
    pub const LEN: usize = N;

    #[must_use]
    pub const fn new_static() -> Self {
        Self { _priv: () }
    }
}

/// Device abstraction for a single WS2812-style LED strip.
///
/// Each Pico contains two (Pico 1) or three (Pico 2) PIO units.
/// This driver requires one PIO unit per LED strip. The more complex [led_strip_shared](crate::led_strip::led_strip_shared) can drive up to four strips per PIO.
///
/// # Example
/// ```no_run
/// # #![no_std]
/// # use panic_probe as _;
/// # fn main() {}
/// use device_kit::led_strip::{
///     LedStripSimple,
///     LedStripSimpleStatic,
///     Milliamps,
///     colors,
///     new_simple_strip,
/// };
/// use device_kit::Result;
///
/// async fn example(p: embassy_rp::Peripherals) -> Result<()> {
///     static STRIP_STATIC: LedStripSimpleStatic<8> = LedStripSimpleStatic::new_static();
///     let mut strip = new_simple_strip!(
///         &STRIP_STATIC,  // static resources
///         PIN_2,          // data pin
///         p.PIO0,         // PIO block
///         Milliamps(50)   // max current budget (mA)
///     ).await;
///
///     let mut frame = [colors::BLACK; 8];
///     frame[0] = colors::WHITE;
///     strip.update_pixels(&frame).await?;
///     Ok(())
/// }
/// ```
pub struct LedStripSimple<'d, PIO: Instance, const N: usize> {
    driver: PioWs2812Cpu<'d, PIO, 0, N, Grb>,
    max_brightness: u8,
}

impl<'d, PIO: Instance, const N: usize> LedStripSimple<'d, PIO, N> {
    /// Construct a new inline strip driver from shared bus/state machine and pin.
    pub(crate) fn new(
        strip_static: &'static LedStripSimpleStatic<N>,
        bus: &'static PioBus<'static, PIO>,
        sm: StateMachine<'static, PIO, 0>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_brightness: u8,
    ) -> Self {
        let _ = strip_static; // marker to match Device/Static pattern
        let driver = new_driver_grb::<PIO, 0, N>(bus, sm, pin);
        Self {
            driver,
            max_brightness,
        }
    }

    /// Update all pixels at once.
    ///
    /// See [`LedStripSimple`] for the usage example.
    pub async fn update_pixels(&mut self, pixels: &[Rgb; N]) -> Result<()> {
        let mut frame = *pixels;
        apply_max_brightness(&mut frame, self.max_brightness);
        self.driver.write(&frame).await;
        Ok(())
    }
}

impl<const N: usize> LedStripSimple<'static, embassy_rp::peripherals::PIO0, N> {
    /// Builds a `LedStripSimple` on PIO0/SM0.
    ///
    /// Each Pico contains two (Pico 1) or three (Pico 2) PIO units; this driver requires one PIO unit per LED strip. The more complex [led_strip_shared](crate::led_strip::led_strip_shared) can drive up to four strips per PIO.
    ///
    /// See [`LedStripSimple`] for the usage example.
    pub async fn new_pio0(
        strip_static: &'static LedStripSimpleStatic<N>,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
    ) -> Self {
        let max_brightness = max_brightness_for::<N>(max_current);
        let (bus, sm) = init_pio0(pio);
        let mut strip = LedStripSimple::new(strip_static, bus, sm, pin, max_brightness);
        // Initialize with blank frame to ensure LEDs are ready
        let blank = [Rgb::new(0, 0, 0); N];
        strip.update_pixels(&blank).await.ok();
        strip
    }
}

impl<const N: usize> LedStripSimple<'static, embassy_rp::peripherals::PIO1, N> {
    /// Builds a `LedStripSimple` on PIO1/SM0.
    ///
    /// Each Pico contains two (Pico 1) or three (Pico 2) PIO units; this driver requires one PIO unit per LED strip. The more complex [led_strip_shared](crate::led_strip::led_strip_shared) can drive up to four strips per PIO.
    ///
    /// See [`LedStripSimple`] for the usage example.
    pub async fn new_pio1(
        strip_static: &'static LedStripSimpleStatic<N>,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO1>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
    ) -> Self {
        let max_brightness = max_brightness_for::<N>(max_current);
        let (bus, sm) = init_pio1(pio);
        let mut strip = LedStripSimple::new(strip_static, bus, sm, pin, max_brightness);
        // Initialize with blank frame to ensure LEDs are ready
        let blank = [Rgb::new(0, 0, 0); N];
        strip.update_pixels(&blank).await.ok();
        strip
    }
}

#[cfg(feature = "pico2")]
impl<const N: usize> LedStripSimple<'static, embassy_rp::peripherals::PIO2, N> {
    /// Builds a `LedStripSimple` on PIO2/SM0.
    ///
    /// Each Pico contains two (Pico 1) or three (Pico 2) PIO units; this driver requires one PIO unit per LED strip. The more complex [led_strip_shared](crate::led_strip::led_strip_shared) can drive up to four strips per PIO.
    ///
    /// See [`LedStripSimple`] for the usage example.
    pub async fn new_pio2(
        strip_static: &'static LedStripSimpleStatic<N>,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO2>,
        pin: embassy_rp::Peri<'static, impl PioPin>,
        max_current: Milliamps,
    ) -> Self {
        let max_brightness = max_brightness_for::<N>(max_current);
        let (bus, sm) = init_pio2(pio);
        let mut strip = LedStripSimple::new(strip_static, bus, sm, pin, max_brightness);
        // Initialize with blank frame to ensure LEDs are ready
        let blank = [Rgb::new(0, 0, 0); N];
        strip.update_pixels(&blank).await.ok();
        strip
    }
}

#[doc(hidden)]
#[macro_export]
/// Macro wrapper that routes to `new_pio0`/`new_pio1`/`new_pio2` and fails fast if PIO2 is used on Pico 1.
/// See the usage example on [`LedStripSimple`].
macro_rules! new_simple_strip {
    (
        $strip_static:expr,
        $pin:ident,
        $peripherals:ident . PIO0,
        $max_current:expr
    ) => {
        $crate::led_strip::LedStripSimple::new_pio0(
            $strip_static,
            $peripherals.PIO0,
            $peripherals.$pin,
            $max_current,
        )
    };
    (
        $strip_static:expr,
        $pin:ident,
        $peripherals:ident . PIO1,
        $max_current:expr
    ) => {
        $crate::led_strip::LedStripSimple::new_pio1(
            $strip_static,
            $peripherals.PIO1,
            $peripherals.$pin,
            $max_current,
        )
    };
    (
        $strip_static:expr,
        $pin:ident,
        $peripherals:ident . PIO2,
        $max_current:expr
    ) => {{
        #[cfg(feature = "pico2")]
        {
            $crate::led_strip::LedStripSimple::new_pio2(
                $strip_static,
                $peripherals.PIO2,
                $peripherals.$pin,
                $max_current,
            )
        }
        #[cfg(not(feature = "pico2"))]
        {
            compile_error!("PIO2 is only available on Pico 2 (rp235x); enable the pico2 feature or choose PIO0/PIO1");
        }
    }};
}

/// Macro wrapper that routes to `new_pio0`/`new_pio1`/`new_pio2` and fails fast if PIO2 is used on Pico 1.
/// See the usage example on [`LedStripSimple`].
#[doc(inline)]
pub use new_simple_strip;
