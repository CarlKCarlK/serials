// A device abstraction for WS2812-style LED strips.
//
// See [`LedStrip`] for the main usage example.
// cmk000 why is this file named this?

use core::cell::RefCell;
use embassy_futures::select::{Either, select};
use embassy_rp::pio::{Common, Instance};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use embassy_sync::once_lock::OnceLock;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use smart_leds::RGB8;

use crate::Result;

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

/// Frame of `Rgb` values for a 1D LED strip.
///
/// Use [`Frame::new`] for a blank frame or [`Frame::filled`] for a solid color. Frames deref to
/// `[Rgb; N]`, so you can mutate pixels directly before passing them to [`LedStrip::write_frame`].
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// # use device_kit::led_strip::Frame;
/// # use device_kit::led_strip::colors;
/// # fn example() {
/// let mut frame = Frame::<8>::new();
/// frame[0] = colors::RED;
/// frame[7] = colors::GREEN;
/// let _ = frame;
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Frame<const N: usize>(pub [Rgb; N]);

impl<const N: usize> Frame<N> {
    /// Create a new blank (all black) frame.
    #[must_use]
    pub const fn new() -> Self {
        Self([Rgb::new(0, 0, 0); N])
    }

    /// Create a frame filled with a single color.
    #[must_use]
    pub const fn filled(color: Rgb) -> Self {
        Self([color; N])
    }

    /// Get the number of LEDs in this frame.
    #[must_use]
    pub const fn len() -> usize {
        N
    }
}

impl<const N: usize> core::ops::Deref for Frame<N> {
    type Target = [Rgb; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> core::ops::DerefMut for Frame<N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const N: usize> From<[Rgb; N]> for Frame<N> {
    fn from(array: [Rgb; N]) -> Self {
        Self(array)
    }
}

impl<const N: usize> From<Frame<N>> for [Rgb; N] {
    fn from(frame: Frame<N>) -> Self {
        frame.0
    }
}

impl<const N: usize> Default for Frame<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PIO Bus - Shared PIO resource for multiple LED strips
// ============================================================================

/// Trait for PIO peripherals that can be used with LED strips.
///
/// This trait is automatically implemented by the `led_strips!` macro
/// for the PIO peripheral specified in the macro invocation.
#[doc(hidden)] // Required pub for macro expansion in downstream crates
pub trait LedStripPio: Instance {
    /// The interrupt binding type for this PIO
    type Irqs: embassy_rp::interrupt::typelevel::Binding<
            <Self as Instance>::Interrupt,
            embassy_rp::pio::InterruptHandler<Self>,
        >;

    /// Get the interrupt configuration
    fn irqs() -> Self::Irqs;
}
/// A state machine bundled with its PIO bus.
///
/// This is returned by `pio_split!` and passed to strip constructors.
#[doc(hidden)] // Support type for macro-generated strip types; not intended as surface API
pub struct PioStateMachine<PIO: Instance + 'static, const SM: usize> {
    bus: &'static PioBus<'static, PIO>,
    sm: embassy_rp::pio::StateMachine<'static, PIO, SM>,
}
// cmk should spell out sm and name bus pio_bus, this this be PioBusStateMachine?ks

impl<PIO: Instance + 'static, const SM: usize> PioStateMachine<PIO, SM> {
    #[doc(hidden)]
    pub fn new(
        bus: &'static PioBus<'static, PIO>,
        sm: embassy_rp::pio::StateMachine<'static, PIO, SM>,
    ) -> Self {
        Self { bus, sm }
    }

    #[doc(hidden)]
    pub fn bus(&self) -> &'static PioBus<'static, PIO> {
        self.bus
    }

    #[doc(hidden)]
    pub fn into_parts(
        self,
    ) -> (
        &'static PioBus<'static, PIO>,
        embassy_rp::pio::StateMachine<'static, PIO, SM>,
    ) {
        (self.bus, self.sm)
    }
}
/// Shared PIO bus that manages the Common resource and WS2812 program
#[doc(hidden)] // Support type for macro-generated strip types; not intended as surface API
pub struct PioBus<'d, PIO: Instance> {
    common: Mutex<CriticalSectionRawMutex, RefCell<Common<'d, PIO>>>,
    ws2812_program: OnceLock<PioWs2812Program<'d, PIO>>,
}

impl<'d, PIO: Instance> PioBus<'d, PIO> {
    /// Create a new PIO bus with the given Common resource
    pub fn new(common: Common<'d, PIO>) -> Self {
        Self {
            common: Mutex::new(RefCell::new(common)),
            ws2812_program: OnceLock::new(),
        }
    }

    /// Get or initialize the WS2812 program (only loaded once)
    pub fn get_program(&'static self) -> &'static PioWs2812Program<'d, PIO> {
        self.ws2812_program.get_or_init(|| {
            self.common.lock(|common_cell: &RefCell<Common<'d, PIO>>| {
                let mut common = common_cell.borrow_mut();
                PioWs2812Program::new(&mut *common)
            })
        })
    }

    /// Access the common resource for initializing a driver
    pub fn with_common<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Common<'d, PIO>) -> R,
    {
        self.common.lock(|common_cell: &RefCell<Common<'d, PIO>>| {
            let mut common = common_cell.borrow_mut();
            f(&mut *common)
        })
    }
}

// ============================================================================
// LED Strip Command Channel and Static
// ============================================================================

#[doc(hidden)] // Required pub for macro expansion in downstream crates
pub type LedStripCommands<const N: usize> = EmbassyChannel<CriticalSectionRawMutex, Frame<N>, 2>;

#[doc(hidden)] // Required pub for macro expansion in downstream crates
pub type LedStripCommandSignal<const N: usize, const MAX_FRAMES: usize> =
    Signal<CriticalSectionRawMutex, Command<N, MAX_FRAMES>>;

#[doc(hidden)] // Required pub for macro expansion in downstream crates
pub type LedStripCompletionSignal = Signal<CriticalSectionRawMutex, ()>;

#[doc(hidden)]
// Command for the LED strip animation loop.
#[derive(Clone)]
pub enum Command<const N: usize, const MAX_FRAMES: usize> {
    DisplayStatic(Frame<N>),
    Animate(Vec<(Frame<N>, Duration), MAX_FRAMES>),
}

/// Static used to construct LED strip instances with animation support.
#[doc(hidden)] // Must be pub for method signatures and macro expansion in downstream crates
pub struct LedStripStatic<const N: usize, const MAX_FRAMES: usize> {
    command_signal: LedStripCommandSignal<N, MAX_FRAMES>,
    completion_signal: LedStripCompletionSignal,
    commands: LedStripCommands<N>,
}

impl<const N: usize, const MAX_FRAMES: usize> LedStripStatic<N, MAX_FRAMES> {
    /// Creates static resources.
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            command_signal: Signal::new(),
            completion_signal: Signal::new(),
            commands: LedStripCommands::new(),
        }
    }

    pub fn command_signal(&'static self) -> &'static LedStripCommandSignal<N, MAX_FRAMES> {
        &self.command_signal
    }

    pub fn completion_signal(&'static self) -> &'static LedStripCompletionSignal {
        &self.completion_signal
    }

    pub fn commands(&'static self) -> &'static LedStripCommands<N> {
        &self.commands
    }
}

/// Device abstraction for WS2812-style LED strips created by [`led_strips!`] (multiple strips can share one PIO).
///
/// This type is used through macro-generated wrapper types that deref to `LedStrip`.
///
/// The [`led_strips!`] macro generates wrapper types with associated constants
/// (`LEN`, `MAX_BRIGHTNESS`) and handles all resource allocation and driver spawning.
pub struct LedStrip<const N: usize, const MAX_FRAMES: usize> {
    command_signal: &'static LedStripCommandSignal<N, MAX_FRAMES>,
    completion_signal: &'static LedStripCompletionSignal,
}

impl<const N: usize, const MAX_FRAMES: usize> LedStrip<N, MAX_FRAMES> {
    /// Creates LED strip resources.
    #[must_use]
    pub const fn new_static() -> LedStripStatic<N, MAX_FRAMES> {
        LedStripStatic::new_static()
    }

    /// Creates a new LED strip controller bound to the given static resources.
    pub fn new(led_strip_static: &'static LedStripStatic<N, MAX_FRAMES>) -> Result<Self> {
        Ok(Self {
            command_signal: led_strip_static.command_signal(),
            completion_signal: led_strip_static.completion_signal(),
        })
    }

    /// Writes a full frame to the LED strip and displays it until the next command.
    pub async fn write_frame(&self, frame: Frame<N>) -> Result<()> {
        self.command_signal.signal(Command::DisplayStatic(frame));
        self.completion_signal.wait().await;
        Ok(())
    }

    /// Loop through a sequence of animation frames until interrupted by another command.
    ///
    /// Each frame is a tuple of `(Frame, Duration)`. Accepts arrays, `Vec`s, or any
    /// iterator that produces `(Frame, Duration)` tuples.
    pub async fn animate(
        &self,
        frames: impl IntoIterator<Item = (Frame<N>, Duration)>,
    ) -> Result<()> {
        assert!(
            MAX_FRAMES > 0,
            "max_frames must be positive for LED strip animations"
        );
        let mut sequence: Vec<(Frame<N>, Duration), MAX_FRAMES> = Vec::new();
        for (frame, duration) in frames {
            assert!(
                duration.as_micros() > 0,
                "animation frame duration must be positive"
            );
            sequence
                .push((frame, duration))
                .expect("animation sequence fits within MAX_FRAMES");
        }
        assert!(
            !sequence.is_empty(),
            "animation requires at least one frame"
        );
        self.command_signal.signal(Command::Animate(sequence));
        self.completion_signal.wait().await;
        Ok(())
    }
}

#[doc(hidden)] // Required pub for macro expansion in downstream crates
pub async fn led_strip_animation_loop<
    PIO,
    const SM: usize,
    const N: usize,
    const MAX_FRAMES: usize,
    ORDER,
>(
    mut driver: PioWs2812<'static, PIO, SM, N, ORDER>,
    command_signal: &'static LedStripCommandSignal<N, MAX_FRAMES>,
    completion_signal: &'static LedStripCompletionSignal,
    combo_table: &'static [u8; 256],
) -> !
where
    PIO: Instance,
    ORDER: embassy_rp::pio_programs::ws2812::RgbColorOrder,
{
    loop {
        let command = command_signal.wait().await;
        command_signal.reset();

        match command {
            Command::DisplayStatic(frame) => {
                let mut corrected_frame = frame;
                apply_correction(&mut corrected_frame, combo_table);
                driver.write(&corrected_frame).await;
                completion_signal.signal(());
            }
            Command::Animate(frames) => {
                let next_command = run_frame_animation(
                    &mut driver,
                    frames,
                    command_signal,
                    completion_signal,
                    combo_table,
                )
                .await;
                command_signal.reset();
                match next_command {
                    Command::DisplayStatic(frame) => {
                        let mut corrected_frame = frame;
                        apply_correction(&mut corrected_frame, combo_table);
                        driver.write(&corrected_frame).await;
                        completion_signal.signal(());
                    }
                    Command::Animate(_) => {
                        // Loop back to process new animation
                        continue;
                    }
                }
            }
        }
    }
}

async fn run_frame_animation<PIO, const SM: usize, const N: usize, const MAX_FRAMES: usize, ORDER>(
    driver: &mut PioWs2812<'static, PIO, SM, N, ORDER>,
    frames: Vec<(Frame<N>, Duration), MAX_FRAMES>,
    command_signal: &'static LedStripCommandSignal<N, MAX_FRAMES>,
    completion_signal: &'static LedStripCompletionSignal,
    combo_table: &'static [u8; 256],
) -> Command<N, MAX_FRAMES>
where
    PIO: Instance,
    ORDER: embassy_rp::pio_programs::ws2812::RgbColorOrder,
{
    completion_signal.signal(());

    loop {
        for (frame, duration) in &frames {
            let mut corrected_frame = *frame;
            apply_correction(&mut corrected_frame, combo_table);
            driver.write(&corrected_frame).await;

            match select(command_signal.wait(), Timer::after(*duration)).await {
                Either::First(new_command) => {
                    return new_command;
                }
                Either::Second(()) => continue,
            }
        }
    }
}

fn apply_correction<const N: usize>(frame: &mut Frame<N>, combo_table: &[u8; 256]) {
    for color in frame.iter_mut() {
        *color = Rgb::new(
            combo_table[usize::from(color.r)],
            combo_table[usize::from(color.g)],
            combo_table[usize::from(color.b)],
        );
    }
}

// ============================================================================
// Macro: led_strips - Creates interrupts, PIO bus, and LED strips
// ============================================================================

/// Creates PIO-based LED strip configurations with automatic brightness limiting.
///
/// This macro generates all the necessary code to create multiple WS2812-style LED strips
/// using a single PIO peripheral. It handles interrupt bindings, PIO bus sharing, and
/// per-strip brightness limiting based on current budget.
///
/// The macro generates:
/// - A `pio0_split()` (or `pio1_split()`, `pio2_split()`) function that splits the PIO
/// - One type per strip with `new_static()` and `new()` constructors
///
/// Each generated type dereferences to [`LedStrip`](crate::led_strip::LedStrip)
/// so you can call `write_frame` directly. The type name is exactly the identifier
/// you supply to the macro; use CamelCase there to satisfy linting and keep types
/// recognizable (e.g., `Gpio2LedStrip`).
///
/// The split functions use the `LedStripPio` trait (implemented for PIO0, PIO1, PIO2)
/// to get interrupt bindings, similar to how wifi_auto handles PIO generics.
///
/// # Example
/// ```no_run
/// #![no_std]
/// use panic_probe as _;
/// // Requires target support and macro imports; no_run to avoid hardware access in doctests.
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! led_strips {
    // Internal: full expansion with all fields specified
    (@__expand
        pio: $pio:ident,
        group: $group:ident,
        strips: [
            $(
                $label:ident {
                    sm: $sm_index:expr,
                    dma: $dma:ident,
                    pin: $pin:ident,
                    len: $len:expr,
                    max_current: $max_current:expr,
                    gamma: $gamma:expr,
                    max_frames: $max_frames:expr
                    $(,
                        led2d: {
                            width: $led2d_width:expr,
                            height: $led2d_height:expr,
                            led_layout: $led2d_led_layout:ident $( ( $($led2d_led_layout_args:tt)* ) )?,
                            max_frames: $led2d_max_frames:expr,
                            font: $led2d_font:ident $(,)?
                        }
                    )?
                }
            ),+ $(,)?
        ]
    ) => {
        // Use crate-level PIO interrupt bindings (Pio0Irqs, Pio1Irqs, Pio2Irqs)
        paste::paste! {
            // Create the PIO bus
            #[allow(non_upper_case_globals)]
            static [<$pio _BUS>]: ::static_cell::StaticCell<
                $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>
            > = ::static_cell::StaticCell::new();

            /// Split the PIO into bus and state machines.
            ///
            /// Returns 4 StateMachines (one for each SM)
            #[allow(dead_code)]
            pub fn [<$pio:lower _split>](
                pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
            ) -> (
                $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, 0>,
                $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, 1>,
                $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, 2>,
                $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, 3>,
            ) {
                let ::embassy_rp::pio::Pio { common, sm0, sm1, sm2, sm3, .. } =
                    ::embassy_rp::pio::Pio::new(pio, <::embassy_rp::peripherals::$pio as $crate::led_strip::LedStripPio>::irqs());
                let pio_bus = [<$pio _BUS>].init_with(|| {
                    $crate::led_strip::PioBus::new(common)
                });
                (
                    $crate::led_strip::PioStateMachine::new(pio_bus, sm0),
                    $crate::led_strip::PioStateMachine::new(pio_bus, sm1),
                    $crate::led_strip::PioStateMachine::new(pio_bus, sm2),
                    $crate::led_strip::PioStateMachine::new(pio_bus, sm3),
                )
            }


        }

        paste::paste! {
            // Create strip types
            $(
                #[doc = concat!(
                    "LED strip wrapper generated by [`led_strips!`].\n\n",
                    "Derefs to [`LedStrip`] for all operations. ",
                    "Created with [`", stringify!($group), "::new`]."
                )]
                pub struct [<$label:camel LedStrip>] {
                    strip: $crate::led_strip::LedStrip<{ $len }, { $max_frames }>,
                }

                impl [<$label:camel LedStrip>] {
                    pub const LEN: usize = $len;
                    pub const MAX_FRAMES: usize = $max_frames;

                    // Calculate max brightness from current budget
                    // Each WS2812B LED draws ~60mA at full brightness
                    /// cmk00 OK to assume 60 mA per LED
                    const WORST_CASE_MA: u32 = ($len as u32) * 60;
                    pub const MAX_BRIGHTNESS: u8 =
                        $max_current.max_brightness(Self::WORST_CASE_MA);

                    // Combined gamma correction and brightness scaling table
                    const COMBO_TABLE: [u8; 256] = $crate::led_strip::gamma::generate_combo_table($gamma, Self::MAX_BRIGHTNESS);

                    pub(crate) const fn new_static() -> $crate::led_strip::LedStripStatic<{ $len }, { $max_frames }> {
                        $crate::led_strip::LedStrip::new_static()
                    }

                    pub fn new(
                        state_machine: $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, $sm_index>,
                        dma: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>>,
                        pin: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>>,
                        spawner: ::embassy_executor::Spawner,
                    ) -> $crate::Result<&'static Self> {
                        static STRIP_STATIC: $crate::led_strip::LedStripStatic<{ $len }, { $max_frames }> = [<$label:camel LedStrip>]::new_static();
                        static STRIP_CELL: ::static_cell::StaticCell<[<$label:camel LedStrip>]> = ::static_cell::StaticCell::new();
                        let (bus, sm) = state_machine.into_parts();
                        let token = [<$group:snake _ $label:snake _animation_task>](
                            bus,
                            sm,
                            dma.into(),
                            pin.into(),
                            STRIP_STATIC.command_signal(),
                            STRIP_STATIC.completion_signal(),
                        )
                        .map_err($crate::Error::TaskSpawn)?;
                        spawner.spawn(token);
                        let strip = $crate::led_strip::LedStrip::new(&STRIP_STATIC)?;
                        let instance = STRIP_CELL.init(Self { strip });
                        Ok(instance)
                    }
                }

                impl ::core::ops::Deref for [<$label:camel LedStrip>] {
                    type Target = $crate::led_strip::LedStrip<{ $len }, { $max_frames }>;

                    fn deref(&self) -> &Self::Target {
                        &self.strip
                    }
                }

                #[cfg(not(feature = "host"))]
                impl $crate::led2d::WriteFrame<{ $len }> for [<$label:camel LedStrip>] {
                    async fn write_frame(&self, frame: $crate::led_strip::Frame<{ $len }>) -> $crate::Result<()> {
                        self.strip.write_frame(frame).await
                    }
                }

                #[::embassy_executor::task]
                async fn [<$group:snake _ $label:snake _animation_task>](
                    bus: &'static $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>,
                    sm: ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, $sm_index>,
                    dma: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>,
                    pin: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>,
                    command_signal: &'static $crate::led_strip::LedStripCommandSignal<{ $len }, { $max_frames }>,
                    completion_signal: &'static $crate::led_strip::LedStripCompletionSignal,
                ) -> ! {
                    let program = bus.get_program();
                    let driver = bus.with_common(|common| {
                        ::embassy_rp::pio_programs::ws2812::PioWs2812::<
                            ::embassy_rp::peripherals::$pio,
                            $sm_index,
                            { $len },
                            _
                        >::new(common, sm, dma, pin, program)
                    });
                    $crate::led_strip::led_strip_animation_loop::<
                        ::embassy_rp::peripherals::$pio,
                        $sm_index,
                        { $len },
                        { $max_frames },
                        _
                    >(driver, command_signal, completion_signal, &[<$label:camel LedStrip>]::COMBO_TABLE).await
                }

                $(
                    paste::paste! {
                        #[cfg(not(feature = "host"))]
                        $crate::led2d::led2d_from_strip! {
                            pub [<$label:camel LedStripLed2d>],
                            strip_type: [<$label:camel LedStrip>],
                            width: $led2d_width,
                            height: $led2d_height,
                            led_layout: $led2d_led_layout $( ( $($led2d_led_layout_args)* ) )?,
                            max_frames: $led2d_max_frames,
                            font: $led2d_font,
                        }

                        #[cfg(not(feature = "host"))]
                        impl [<$label:camel LedStrip>] {
                            pub fn new_led2d(
                                state_machine: $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, $sm_index>,
                                dma: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>>,
                                pin: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>>,
                                spawner: ::embassy_executor::Spawner,
                            ) -> $crate::Result<[<$label:camel LedStripLed2d>]> {
                                let strip = Self::new(state_machine, dma, pin, spawner)?;
                                [<$label:camel LedStripLed2d>]::from_strip(strip, spawner)
                            }
                        }
                    }
                )?
            )+

            // Generate the group marker struct with new() constructor
            pub struct $group;

            impl $group {
                #[allow(clippy::too_many_arguments)]
                pub fn new(
                    pio: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>>,
                    $(
                        [<$label _dma>]: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>>,
                        [<$label _pin>]: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>>,
                    )+
                    spawner: ::embassy_executor::Spawner,
                ) -> $crate::Result<(
                    $( &'static [<$label:camel LedStrip>], )+
                )> {
                    // Inline PIO splitting
                    let pio_peri = pio.into();
                    let ::embassy_rp::pio::Pio { common, sm0, sm1, sm2, sm3, .. } =
                        ::embassy_rp::pio::Pio::new(pio_peri, <::embassy_rp::peripherals::$pio as $crate::led_strip::LedStripPio>::irqs());
                    let pio_bus = [<$pio _BUS>].init_with(|| {
                        $crate::led_strip::PioBus::new(common)
                    });

                    // Create individual state machine wrappers
                    let sm0_wrapped = $crate::led_strip::PioStateMachine::new(pio_bus, sm0);
                    let sm1_wrapped = $crate::led_strip::PioStateMachine::new(pio_bus, sm1);
                    let sm2_wrapped = $crate::led_strip::PioStateMachine::new(pio_bus, sm2);
                    let sm3_wrapped = $crate::led_strip::PioStateMachine::new(pio_bus, sm3);

                    // Construct each strip with the appropriate SM
                    Ok((
                        $(
                            [<$label:camel LedStrip>]::new(
                                led_strips!(@__select_sm $sm_index, sm0_wrapped, sm1_wrapped, sm2_wrapped, sm3_wrapped),
                                [<$label _dma>],
                                [<$label _pin>],
                                spawner
                            )?,
                        )+
                    ))
                }
            }
        }
    };

    // Helper to select the right SM based on index
    (@__select_sm 0, $sm0:ident, $sm1:ident, $sm2:ident, $sm3:ident) => { $sm0 };
    (@__select_sm 1, $sm0:ident, $sm1:ident, $sm2:ident, $sm3:ident) => { $sm1 };
    (@__select_sm 2, $sm0:ident, $sm1:ident, $sm2:ident, $sm3:ident) => { $sm2 };
    (@__select_sm 3, $sm0:ident, $sm1:ident, $sm2:ident, $sm3:ident) => { $sm3 };

    // Entry point with explicit pio and group syntax
    (
        pio: $pio:ident,
        $group:ident {
            $( $label:ident: { $($fields:tt)* } ),+ $(,)?
        }
    ) => {
        led_strips! {
            @__with_defaults
            pio: $pio,
            group: $group,
            sm_counter: 0,
            strips_out: [],
            strips_in: [ $( $label: { $($fields)* } ),+ ]
        }
    };

    // Entry point without pio (defaults to PIO0) with group syntax
    (
        $group:ident {
            $( $label:ident: { $($fields:tt)* } ),+ $(,)?
        }
    ) => {
        led_strips! {
            @__with_defaults
            pio: PIO0,
            group: $group,
            sm_counter: 0,
            strips_out: [],
            strips_in: [ $( $label: { $($fields)* } ),+ ]
        }
    };

    // Process strips one at a time, adding defaults
    (@__with_defaults
        pio: $pio:ident,
        group: $group:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_in: [ $label:ident: { $($fields:tt)* } $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($($rest)*)? ],
            label: $label,
            group: $group,
            pin: __MISSING_PIN__,
            dma: __DEFAULT_DMA__,
            len: __MISSING_LEN__,
            max_current: $crate::led_strip::Current::Unlimited,
            gamma: $crate::led_strip::gamma::Gamma::Linear,
            max_frames: 32,
            led2d: __NONE__,
            fields: [ $($fields)* ]
        }
    };

    // All strips processed, call the main implementation
    (@__with_defaults
        pio: $pio:ident,
        group: $group:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_in: []
    ) => {
        led_strips! {
            @__expand
            pio: $pio,
            group: $group,
            strips: [ $($out)* ]
        }
    };

    // Parse fields for a single strip
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: [ pin: $new_pin:ident $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $new_pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: [ dma: $new_dma:ident $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: $new_dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: [ len: $new_len:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: $dma,
            len: $new_len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: [ max_current: $new_max_current:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $new_max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: [ gamma: $new_gamma:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $new_gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: [ max_frames: $new_max_frames:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $new_max_frames,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: __NONE__,
        fields: [ led2d: { $($led2d_fields:tt)* } $(, $($rest:tt)* )? ]
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: __HAS_LED2D__ { $($led2d_fields)* },
            fields: [ $($($rest)*)? ]
        }
    };

    // Done parsing fields, add strip to output and continue
    // Special case: convert __DEFAULT_DMA__ to actual DMA channel based on sm
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: 0,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:ident,
        dma: __DEFAULT_DMA__,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: []
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: 0,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: DMA_CH0,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: []
        }
    };
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: 1,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:ident,
        dma: __DEFAULT_DMA__,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: []
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: 1,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: DMA_CH1,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: []
        }
    };
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: 2,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:ident,
        dma: __DEFAULT_DMA__,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: []
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: 2,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: DMA_CH2,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: []
        }
    };
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: 3,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:ident,
        dma: __DEFAULT_DMA__,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: $led2d:tt,
        fields: []
    ) => {
        led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: 3,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            label: $label,
            group: $group,
            pin: $pin,
            dma: DMA_CH3,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            led2d: $led2d,
            fields: []
        }
    };

    // Done parsing fields, add strip to output and continue
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: __NONE__,
        fields: []
    ) => {
        led_strips! {
            @__inc_counter
            pio: $pio,
            group: $group,
            sm: $sm,
            strips_out: [
                $($out)*
                $label {
                    sm: $sm,
                    dma: $dma,
                    pin: $pin,
                    len: $len,
                    max_current: $max_current,
                    gamma: $gamma,
                    max_frames: $max_frames
                },
            ],
            strips_in: [ $($remaining)* ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        label: $label:ident,
        group: $group:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        led2d: __HAS_LED2D__ { $($led2d_fields:tt)* },
        fields: []
    ) => {
        led_strips! {
            @__inc_counter
            pio: $pio,
            group: $group,
            sm: $sm,
            strips_out: [
                $($out)*
                $label {
                    sm: $sm,
                    dma: $dma,
                    pin: $pin,
                    len: $len,
                    max_current: $max_current,
                    gamma: $gamma,
                    max_frames: $max_frames,
                    led2d: { $($led2d_fields)* }
                },
            ],
            strips_in: [ $($remaining)* ]
        }
    };
    // Increment counter by expanding to literal numbers
    (@__inc_counter pio: $pio:ident, group: $group:ident, sm: 0, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        led_strips! { @__with_defaults pio: $pio, group: $group, sm_counter: 1, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
    (@__inc_counter pio: $pio:ident, group: $group:ident, sm: 1, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        led_strips! { @__with_defaults pio: $pio, group: $group, sm_counter: 2, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
    (@__inc_counter pio: $pio:ident, group: $group:ident, sm: 2, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        led_strips! { @__with_defaults pio: $pio, group: $group, sm_counter: 3, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
    (@__inc_counter pio: $pio:ident, group: $group:ident, sm: 3, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        led_strips! { @__with_defaults pio: $pio, group: $group, sm_counter: 4, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
}

/// Used with [`led_strips!`] to split a PIO peripheral into 4 state machines.
///
/// cmk000 users don't need to see the name of hidden functions!
/// Calls the generated `pio0_split`, `pio1_split`, or `pio2_split`
/// function based on the field name in the expression.
///
/// cmk000 want a link not an example
/// # Example
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use embassy_executor::Spawner;
/// use device_kit::led_strip::led_strip;
/// use device_kit::led_strip::Current;
///
/// led_strip! {
///     Gpio0LedStrip {
///         pin: PIN_0,
///         len: 8,
///         max_current: Current::Milliamps(50),
///     }
/// }
///
/// #[embassy_executor::main]
/// async fn main(spawner: Spawner) {
///     let p = embassy_rp::init(Default::default());
///     
///     let gpio0_led_strip =
///         Gpio0LedStrip::new(p.PIO0, p.DMA_CH0, p.PIN_0, spawner).unwrap();
/// }
///
/// ```

/// Simplified macro for defining a single LED strip device (always uses SM0).
///
/// This macro generates a singleton LED strip type that always uses state machine 0 (SM0).
/// Unlike [`led_strips!`], this macro:
/// - Generates a single strip type (not a group)
/// - Always uses SM0 automatically
/// - Returns the strip directly from `new()` (no tuple unpacking)
/// - Hides the PIO split logic inside `new()`
///
/// # Syntax
///
/// ```ignore
/// led_strip! {
///     TypeName {          // Name for the generated strip type
///         pin: PIN_0,     // GPIO pin for LED data (required)
///         len: 8,         // Number of LEDs (required)
///         max_current: Current::Milliamps(500), // Current budget (required)
///     }
/// }
/// ```
///
/// # Optional Fields
///
/// - `pio: PIO1` - PIO peripheral (defaults to PIO0)
/// - `dma: DMA_CH0` - DMA channel (defaults to DMA_CH0)
/// - `gamma: Gamma::Gamma2_2` - Gamma correction (defaults to Gamma2_2)
/// - `max_frames: 16` - Animation frame buffer size (defaults to 16)
///
/// # Generated API
///
/// The macro generates a struct with:
/// - `new(pio, dma, pin, spawner)` - Constructor that handles PIO splitting internally
/// - All methods from [`LedStrip`] via `Deref`
///
/// # Example
///
/// ```ignore
/// use device_kit::led_strip::{led_strip, Current};
/// use embassy_executor::Spawner;
///
/// led_strip! {
///     LedStrip {
///         pio: PIO1,  // Optional, defaults to PIO0
///         pin: PIN_0,
///         len: 8,
///         max_current: Current::Milliamps(50),
///     }
/// }
///
/// #[embassy_executor::main]
/// async fn main(spawner: Spawner) {
///     let p = embassy_rp::init(Default::default());
///     let strip = LedStrip::new(p.PIO1, p.DMA_CH0, p.PIN_0, spawner).unwrap();
///     // Use strip directly - no tuple unpacking needed
/// }
/// ```
#[macro_export]
macro_rules! led_strip {
    // Entry point - name and fields
    (
        $name:ident {
            $($fields:tt)*
        }
    ) => {
        led_strip! {
            @__fill_defaults
            pio: PIO0,
            name: $name,
            pin: _UNSET_,
            dma: DMA_CH0,
            len: _UNSET_,
            max_current: _UNSET_,
            gamma: $crate::led_strip::gamma::Gamma::Gamma2_2,
            max_frames: 16,
            fields: [ $($fields)* ]
        }
    };

    // Fill defaults: pio
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ pio: $new_pio:ident $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $new_pio,
            name: $name,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill defaults: pin
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ pin: $new_pin:ident $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $new_pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill defaults: dma
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ dma: $new_dma:ident $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $pin,
            dma: $new_dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill defaults: len (expression in braces)
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ len: { $new_len:expr } $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $pin,
            dma: $dma,
            len: { $new_len },
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill defaults: len (plain expression)
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ len: $new_len:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $pin,
            dma: $dma,
            len: $new_len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill defaults: max_current
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ max_current: $new_max_current:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $new_max_current,
            gamma: $gamma,
            max_frames: $max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill defaults: gamma
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ gamma: $new_gamma:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $new_gamma,
            max_frames: $max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill defaults: max_frames
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:tt,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: [ max_frames: $new_max_frames:expr $(, $($rest:tt)* )? ]
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            max_frames: $new_max_frames,
            fields: [ $($($rest)*)? ]
        }
    };

    // Fill default max_current if still unset
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        len: $len:expr,
        max_current: _UNSET_,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: []
    ) => {
        led_strip! {
            @__fill_defaults
            pio: $pio,
            name: $name,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $crate::led_strip::Current::Milliamps(250),
            gamma: $gamma,
            max_frames: $max_frames,
            fields: []
        }
    };

    // All fields processed - expand the type
    (@__fill_defaults
        pio: $pio:ident,
        name: $name:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        fields: []
    ) => {
        ::paste::paste! {
            // Create the PIO bus (shared across all SM0 strips using this PIO)
            #[allow(non_upper_case_globals)]
            static [<$pio _BUS>]: ::static_cell::StaticCell<
                $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>
            > = ::static_cell::StaticCell::new();

            /// Split the PIO into bus and state machines.
            ///
            /// Returns SM0 only for single-strip usage.
            #[allow(dead_code)]
            fn [<$pio:lower _split_sm0>](
                pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
            ) -> $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, 0> {
                let ::embassy_rp::pio::Pio { common, sm0, .. } =
                    ::embassy_rp::pio::Pio::new(pio, <::embassy_rp::peripherals::$pio as $crate::led_strip::LedStripPio>::irqs());
                let pio_bus = [<$pio _BUS>].init_with(|| {
                    $crate::led_strip::PioBus::new(common)
                });
                $crate::led_strip::PioStateMachine::new(pio_bus, sm0)
            }

            #[doc = concat!(
                "Singleton LED strip generated by [`led_strip!`].\n\n",
                "Derefs to [`LedStrip`] for all operations. ",
                "Uses state machine 0 (SM0) automatically."
            )]
            pub struct $name {
                strip: $crate::led_strip::LedStrip<{ $len }, { $max_frames }>,
            }

            impl $name {
                pub const LEN: usize = $len;
                pub const MAX_FRAMES: usize = $max_frames;

                // Calculate max brightness from current budget
                const WORST_CASE_MA: u32 = ($len as u32) * 60;
                pub const MAX_BRIGHTNESS: u8 =
                    $max_current.max_brightness(Self::WORST_CASE_MA);

                // Combined gamma correction and brightness scaling table
                const COMBO_TABLE: [u8; 256] = $crate::led_strip::gamma::generate_combo_table($gamma, Self::MAX_BRIGHTNESS);

                /// Create a new LED strip with automatic PIO setup.
                ///
                /// This constructor handles PIO splitting and uses SM0 automatically.
                ///
                /// # Parameters
                ///
                /// - `pio`: PIO peripheral
                /// - `dma`: DMA channel for LED data transfer
                /// - `pin`: GPIO pin for LED data signal
                /// - `spawner`: Task spawner for background operations
                pub fn new(
                    pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
                    dma: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>>,
                    pin: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>>,
                    spawner: ::embassy_executor::Spawner,
                ) -> $crate::Result<&'static Self> {
                    static STRIP_STATIC: $crate::led_strip::LedStripStatic<{ $len }, { $max_frames }> =
                        $crate::led_strip::LedStrip::new_static();
                    static STRIP_CELL: ::static_cell::StaticCell<$name> = ::static_cell::StaticCell::new();

                    let sm0 = [<$pio:lower _split_sm0>](pio);
                    let (bus, sm) = sm0.into_parts();

                    let token = [<$name:snake _animation_task>](
                        bus,
                        sm,
                        dma.into(),
                        pin.into(),
                        STRIP_STATIC.command_signal(),
                        STRIP_STATIC.completion_signal(),
                    )
                    .map_err($crate::Error::TaskSpawn)?;
                    spawner.spawn(token);

                    let strip = $crate::led_strip::LedStrip::new(&STRIP_STATIC)?;
                    let instance = STRIP_CELL.init($name { strip });
                    Ok(instance)
                }
            }

            impl ::core::ops::Deref for $name {
                type Target = $crate::led_strip::LedStrip<{ $len }, { $max_frames }>;

                fn deref(&self) -> &Self::Target {
                    &self.strip
                }
            }

            #[cfg(not(feature = "host"))]
            impl $crate::led2d::WriteFrame<{ $len }> for $name {
                async fn write_frame(&self, frame: $crate::led_strip::Frame<{ $len }>) -> $crate::Result<()> {
                    self.strip.write_frame(frame).await
                }
            }

            #[::embassy_executor::task]
            async fn [<$name:snake _animation_task>](
                bus: &'static $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>,
                sm: ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, 0>,
                dma: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>,
                pin: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>,
                command_signal: &'static $crate::led_strip::LedStripCommandSignal<{ $len }, { $max_frames }>,
                completion_signal: &'static $crate::led_strip::LedStripCompletionSignal,
            ) -> ! {
                let program = bus.get_program();
                let driver = bus.with_common(|common| {
                    ::embassy_rp::pio_programs::ws2812::PioWs2812::<
                        ::embassy_rp::peripherals::$pio,
                        0,
                        { $len },
                        _
                    >::new(common, sm, dma, pin, program)
                });
                $crate::led_strip::led_strip_animation_loop::<
                    ::embassy_rp::peripherals::$pio,
                    0,
                    { $len },
                    { $max_frames },
                    _
                >(driver, command_signal, completion_signal, &$name::COMBO_TABLE).await
            }
        }
    };
}

#[macro_export]
macro_rules! pio_split {
    ($p:ident . PIO0) => {
        pio0_split($p.PIO0)
    };
    ($p:ident . PIO1) => {
        pio1_split($p.PIO1)
    };
    ($p:ident . PIO2) => {
        pio2_split($p.PIO2)
    };
}

pub use pio_split;

// Implement LedStripPio for all PIO peripherals
impl LedStripPio for embassy_rp::peripherals::PIO0 {
    type Irqs = crate::pio_irqs::Pio0Irqs;

    fn irqs() -> Self::Irqs {
        crate::pio_irqs::Pio0Irqs
    }
}

impl LedStripPio for embassy_rp::peripherals::PIO1 {
    type Irqs = crate::pio_irqs::Pio1Irqs;

    fn irqs() -> Self::Irqs {
        crate::pio_irqs::Pio1Irqs
    }
}

#[cfg(feature = "pico2")]
impl LedStripPio for embassy_rp::peripherals::PIO2 {
    type Irqs = crate::pio_irqs::Pio2Irqs;

    fn irqs() -> Self::Irqs {
        crate::pio_irqs::Pio2Irqs
    }
}
