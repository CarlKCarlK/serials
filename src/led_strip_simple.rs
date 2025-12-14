//! A device abstraction for WS2812-style LED strips driven by CPU-fed PIO.
//! See [`LedStrip`] for the main usage example.

use core::cell::RefCell;
use embassy_rp::bind_interrupts;
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
pub use smart_leds::colors;
use static_cell::StaticCell;

use crate::Result;

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

const T1: u8 = 2;
const T2: u8 = 5;
const T3: u8 = 3;
const CYCLES_PER_BIT: u32 = (T1 + T2 + T3) as u32;
const RESET_DELAY_US: u64 = 55;

bind_interrupts!(pub(crate) struct Pio0Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});

bind_interrupts!(pub(crate) struct Pio1Irqs {
    PIO1_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO1>;
});

#[cfg(feature = "pico2")]
bind_interrupts!(pub(crate) struct Pio2Irqs {
    PIO2_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO2>;
});

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

/// Computes a max brightness value given a current budget (mA) and strip length.
#[must_use]
pub const fn max_brightness(len: usize, max_current_ma: u32) -> u8 {
    let worst_case_ma = (len as u32) * 60;
    let scaled = (max_current_ma * 255) / worst_case_ma;
    if scaled > 255 { 255 } else { scaled as u8 }
}

/// Rainbow helper matching the example behavior.
#[must_use]
pub fn wheel(pos: u8) -> Rgb {
    let pos = 255 - pos;
    if pos < 85 {
        rgb(255 - pos * 3, 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        rgb(0, pos * 3, 255 - pos * 3)
    } else {
        let pos = pos - 170;
        rgb(pos * 3, 255 - pos * 3, 0)
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

#[inline]
fn scale_brightness(value: u8, brightness: u8) -> u8 {
    ((u16::from(value) * u16::from(brightness)) / 255) as u8
}

/// Applies a brightness cap to an entire frame in place.
pub fn apply_max_brightness<const N: usize>(frame: &mut [Rgb; N], max_brightness: u8) {
    for color in frame.iter_mut() {
        *color = Rgb::new(
            scale_brightness(color.r, max_brightness),
            scale_brightness(color.g, max_brightness),
            scale_brightness(color.b, max_brightness),
        );
    }
}

/// Static resources for the inline (no-task) strip driver.
pub struct SimpleStripStatic<const N: usize> {
    _priv: (),
}

impl<const N: usize> SimpleStripStatic<N> {
    #[must_use]
    pub const fn new_static() -> Self {
        Self { _priv: () }
    }
}

/// Inline, no-task driver handle with LED-strip-like API.
pub struct SimpleStrip<'d, PIO: Instance, const N: usize> {
    driver: PioWs2812Cpu<'d, PIO, 0, N, Grb>,
    max_brightness: u8,
}

impl<'d, PIO: Instance, const N: usize> SimpleStrip<'d, PIO, N> {
    /// Construct a new inline strip driver from shared bus/state machine and pin.
    pub(crate) fn new(
        strip_static: &'static SimpleStripStatic<N>,
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

    /// Update all pixels at once, applying brightness cap.
    pub async fn update_pixels(&mut self, pixels: &[Rgb; N]) -> Result<()> {
        let mut frame = *pixels;
        apply_max_brightness(&mut frame, self.max_brightness);
        self.driver.write(&frame).await;
        Ok(())
    }
}

/// Convenience constructor that binds PIO0, SM0, and the pin using the internal IRQ helper.
/// Convenience constructor that binds PIO0, SM0, and the pin; derives brightness from current budget.
pub fn new_pio0<const N: usize>(
    strip_static: &'static SimpleStripStatic<N>,
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
    max_current_ma: u32,
) -> SimpleStrip<'static, embassy_rp::peripherals::PIO0, N> {
    let max_brightness = max_brightness(N, max_current_ma);
    new_pio0_with_brightness(strip_static, pio, pin, max_brightness)
}

/// Convenience constructor that binds PIO1, SM0, and the pin; derives brightness from current budget.
pub fn new_pio1<const N: usize>(
    strip_static: &'static SimpleStripStatic<N>,
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO1>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
    max_current_ma: u32,
) -> SimpleStrip<'static, embassy_rp::peripherals::PIO1, N> {
    let max_brightness = max_brightness(N, max_current_ma);
    new_pio1_with_brightness(strip_static, pio, pin, max_brightness)
}

#[cfg(feature = "pico2")]
/// Convenience constructor that binds PIO2, SM0, and the pin; derives brightness from current budget.
pub fn new_pio2<const N: usize>(
    strip_static: &'static SimpleStripStatic<N>,
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO2>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
    max_current_ma: u32,
) -> SimpleStrip<'static, embassy_rp::peripherals::PIO2, N> {
    let max_brightness = max_brightness(N, max_current_ma);
    new_pio2_with_brightness(strip_static, pio, pin, max_brightness)
}

/// Variant that accepts an explicit brightness cap (0-255) for PIO0.
pub fn new_pio0_with_brightness<const N: usize>(
    strip_static: &'static SimpleStripStatic<N>,
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
    max_brightness: u8,
) -> SimpleStrip<'static, embassy_rp::peripherals::PIO0, N> {
    let (bus, sm) = init_pio0(pio);
    SimpleStrip::new(strip_static, bus, sm, pin, max_brightness)
}

/// Variant that accepts an explicit brightness cap (0-255) for PIO1.
pub fn new_pio1_with_brightness<const N: usize>(
    strip_static: &'static SimpleStripStatic<N>,
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO1>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
    max_brightness: u8,
) -> SimpleStrip<'static, embassy_rp::peripherals::PIO1, N> {
    let (bus, sm) = init_pio1(pio);
    SimpleStrip::new(strip_static, bus, sm, pin, max_brightness)
}

#[cfg(feature = "pico2")]
/// Variant that accepts an explicit brightness cap (0-255) for PIO2.
pub fn new_pio2_with_brightness<const N: usize>(
    strip_static: &'static SimpleStripStatic<N>,
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO2>,
    pin: embassy_rp::Peri<'static, impl PioPin>,
    max_brightness: u8,
) -> SimpleStrip<'static, embassy_rp::peripherals::PIO2, N> {
    let (bus, sm) = init_pio2(pio);
    SimpleStrip::new(strip_static, bus, sm, pin, max_brightness)
}

/// Macro wrapper that routes to `new_pio0`/`new_pio1`/`new_pio2` and fails fast if PIO2 is used on Pico 1.
#[macro_export]
macro_rules! new_simple_strip {
    ($strip_static:expr, $pin:ident, $peripherals:ident . PIO0, $max_current_ma:expr) => {
        $crate::led_strip_simple::new_pio0(
            $strip_static,
            $peripherals.PIO0,
            $peripherals.$pin,
            $max_current_ma,
        )
    };
    ($strip_static:expr, $pin:ident, $peripherals:ident . PIO1, $max_current_ma:expr) => {
        $crate::led_strip_simple::new_pio1(
            $strip_static,
            $peripherals.PIO1,
            $peripherals.$pin,
            $max_current_ma,
        )
    };
    ($strip_static:expr, $pin:ident, $peripherals:ident . PIO2, $max_current_ma:expr) => {{
        #[cfg(feature = "pico2")]
        {
            $crate::led_strip_simple::new_pio2(
                $strip_static,
                $peripherals.PIO2,
                $peripherals.$pin,
                $max_current_ma,
            )
        }
        #[cfg(not(feature = "pico2"))]
        {
            compile_error!("PIO2 is only available on Pico 2 (rp235x); enable the pico2 feature or choose PIO0/PIO1");
        }
    }};

    // Optional max_current_ma: defaults to full brightness (255) via explicit brightness constructor.
    ($strip_static:expr, $pin:ident, $peripherals:ident . PIO0) => {
        $crate::led_strip_simple::new_pio0_with_brightness(
            $strip_static,
            $peripherals.PIO0,
            $peripherals.$pin,
            255,
        )
    };
    ($strip_static:expr, $pin:ident, $peripherals:ident . PIO1) => {
        $crate::led_strip_simple::new_pio1_with_brightness(
            $strip_static,
            $peripherals.PIO1,
            $peripherals.$pin,
            255,
        )
    };
    ($strip_static:expr, $pin:ident, $peripherals:ident . PIO2) => {{
        #[cfg(feature = "pico2")]
        {
            $crate::led_strip_simple::new_pio2_with_brightness(
                $strip_static,
                $peripherals.PIO2,
                $peripherals.$pin,
                255,
            )
        }
        #[cfg(not(feature = "pico2"))]
        {
            compile_error!("PIO2 is only available on Pico 2 (rp235x); enable the pico2 feature or choose PIO0/PIO1");
        }
    }};
}

pub use crate::new_simple_strip;
