// A device abstraction for WS2812-style LED strips.
//
// See [`LedStrip`] for the main usage example.

use core::cell::RefCell;
use embassy_rp::pio::{Common, Instance};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use embassy_sync::once_lock::OnceLock;
use smart_leds::RGB8;

use crate::Result;

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

// ============================================================================
// PIO Bus - Shared PIO resource for multiple LED strips
// ============================================================================

/// Trait for PIO peripherals that can be used with LED strips.
///
/// This trait is automatically implemented by the `define_led_strips!` macro
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
pub type LedStripCommands<const N: usize> = EmbassyChannel<CriticalSectionRawMutex, [Rgb; N], 2>;

/// Static used to construct LED strip instances.
#[doc(hidden)] // Must be pub for method signatures and macro expansion in downstream crates
pub struct LedStripStatic<const N: usize> {
    commands: LedStripCommands<N>,
}

impl<const N: usize> LedStripStatic<N> {
    /// Creates static resources.
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            commands: LedStripCommands::new(),
        }
    }

    pub fn commands(&'static self) -> &'static LedStripCommands<N> {
        &self.commands
    }
}

/// Device abstraction for WS2812-style LED strips created by [`define_led_strips!`] (multiple strips can share one PIO).
///
/// This type is used through macro-generated wrapper types that deref to `LedStrip`.
///
/// The [`define_led_strips!`] macro generates wrapper types with associated constants
/// (`LEN`, `MAX_BRIGHTNESS`) and handles all resource allocation and driver spawning.
pub struct LedStrip<const N: usize> {
    commands: &'static LedStripCommands<N>,
}

impl<const N: usize> LedStrip<N> {
    /// WS2812B timing: ~30µs per LED + 100µs safety margin
    const WRITE_DELAY_US: u64 = (N as u64 * 30) + 100;

    /// Creates LED strip resources.
    #[must_use]
    pub const fn new_static() -> LedStripStatic<N> {
        LedStripStatic::new_static()
    }

    /// Creates a new LED strip controller bound to the given static resources.
    pub fn new(led_strip_static: &'static LedStripStatic<N>) -> Result<Self> {
        Ok(Self {
            commands: led_strip_static.commands(),
        })
    }

    /// Updates all LEDs at once from the provided array.
    pub async fn update_pixels(&self, pixels: &[Rgb; N]) -> Result<()> {
        // Send entire frame as one message (copy array to send through channel)
        self.commands.send(*pixels).await;

        // Wait for the DMA write to complete
        embassy_time::Timer::after(embassy_time::Duration::from_micros(Self::WRITE_DELAY_US)).await;

        Ok(())
    }
}

#[doc(hidden)] // Required pub for macro expansion in downstream crates
pub async fn led_strip_driver_loop<PIO, const SM: usize, const N: usize, ORDER>(
    mut driver: PioWs2812<'static, PIO, SM, N, ORDER>,
    commands: &'static LedStripCommands<N>,
    combo_table: &'static [u8; 256],
) -> !
where
    PIO: Instance,
    ORDER: embassy_rp::pio_programs::ws2812::RgbColorOrder,
{
    loop {
        let mut frame = commands.receive().await;

        // Apply combined gamma and brightness correction
        for color in frame.iter_mut() {
            *color = Rgb::new(
                combo_table[usize::from(color.r)],
                combo_table[usize::from(color.g)],
                combo_table[usize::from(color.b)],
            );
        }

        driver.write(&frame).await;
    }
}

// ============================================================================
// Macro: define_led_strips - Creates interrupts, PIO bus, and LED strips
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
/// so you can call `update_pixels` directly. The type name is exactly the identifier
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
macro_rules! define_led_strips {
    // Internal: full expansion with all fields specified
    (@__expand
        pio: $pio:ident,
        strips: [
            $(
                $module:ident {
                    sm: $sm_index:expr,
                    dma: $dma:ident,
                    pin: $pin:ident,
                    len: $len:expr,
                    max_current: $max_current:expr,
                    gamma: $gamma:expr
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
                    "LED strip wrapper generated by [`define_led_strips!`].\n\n",
                    "Derefs to [`LedStrip`] for all operations. ",
                    "Created with [`", stringify!($module), "::new`] after splitting the PIO with [`pio_split!`]."
                )]
                pub struct $module {
                    strip: $crate::led_strip::LedStrip<{ $len }>,
                }

                impl $module {
                    pub const LEN: usize = $len;

                    // Calculate max brightness from current budget
                    // Each WS2812B LED draws ~60mA at full brightness
                    /// cmk00 OK to assume 60 mA per LED
                    const WORST_CASE_MA: u32 = ($len as u32) * 60;
                    pub const MAX_BRIGHTNESS: u8 =
                        $max_current.max_brightness(Self::WORST_CASE_MA);

                    // Combined gamma correction and brightness scaling table
                    const COMBO_TABLE: [u8; 256] = $crate::led_strip::gamma::generate_combo_table($gamma, Self::MAX_BRIGHTNESS);

                    pub(crate) const fn new_static() -> $crate::led_strip::LedStripStatic<{ $len }> {
                        $crate::led_strip::LedStrip::new_static()
                    }

                    pub fn new(
                        state_machine: $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, $sm_index>,
                        dma: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>>,
                        pin: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>>,
                        spawner: ::embassy_executor::Spawner,
                    ) -> $crate::Result<&'static Self> {
                        static STRIP_STATIC: $crate::led_strip::LedStripStatic<{ $len }> = $module::new_static();
                        static STRIP_CELL: ::static_cell::StaticCell<$module> = ::static_cell::StaticCell::new();
                        let (bus, sm) = state_machine.into_parts();
                        let token = [<$module:snake _driver>](
                            bus,
                            sm,
                            dma.into(),
                            pin.into(),
                            STRIP_STATIC.commands(),
                        )
                        .map_err($crate::Error::TaskSpawn)?;
                        spawner.spawn(token);
                        let strip = $crate::led_strip::LedStrip::new(&STRIP_STATIC)?;
                        let instance = STRIP_CELL.init(Self { strip });
                        Ok(instance)
                    }
                }

                impl ::core::ops::Deref for $module {
                    type Target = $crate::led_strip::LedStrip<{ $len }>;

                    fn deref(&self) -> &Self::Target {
                        &self.strip
                    }
                }

                #[cfg(not(feature = "host"))]
                impl $crate::led2d::UpdatePixels<{ $len }> for $module {
                    async fn update_pixels(&self, pixels: &[$crate::led_strip::Rgb; { $len }]) -> $crate::Result<()> {
                        self.strip.update_pixels(pixels).await
                    }
                }

                #[::embassy_executor::task]
                async fn [<$module:snake _driver>](
                    bus: &'static $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>,
                    sm: ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, $sm_index>,
                    dma: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>,
                    pin: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>,
                    commands: &'static $crate::led_strip::LedStripCommands<{ $len }>,
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
                    $crate::led_strip::led_strip_driver_loop::<
                        ::embassy_rp::peripherals::$pio,
                        $sm_index,
                        { $len },
                        _
                    >(driver, commands, &$module::COMBO_TABLE).await
                }

                $(
                    paste::paste! {
                        #[cfg(not(feature = "host"))]
                        $crate::led2d::led2d_from_strip! {
                            pub [<$module Led2d>],
                            strip_type: $module,
                            width: $led2d_width,
                            height: $led2d_height,
                            led_layout: $led2d_led_layout $( ( $($led2d_led_layout_args)* ) )?,
                            max_frames: $led2d_max_frames,
                            font: $led2d_font,
                        }

                        #[cfg(not(feature = "host"))]
                        impl $module {
                            pub fn new_led2d(
                                state_machine: $crate::led_strip::PioStateMachine<::embassy_rp::peripherals::$pio, $sm_index>,
                                dma: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>>,
                                pin: impl Into<::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>>,
                                spawner: ::embassy_executor::Spawner,
                            ) -> $crate::Result<[<$module Led2d>]> {
                                let strip = Self::new(state_machine, dma, pin, spawner)?;
                                [<$module Led2d>]::from_strip(strip, spawner)
                            }
                        }
                    }
                )?
            )+
        }
    };

    // Entry point with explicit pio
    (
        pio: $pio:ident,
        $( $module:ident { $($fields:tt)* } ),+ $(,)?
    ) => {
        define_led_strips! {
            @__with_defaults
            pio: $pio,
            sm_counter: 0,
            strips_out: [],
            strips_in: [ $( $module { $($fields)* } ),+ ]
        }
    };

    // Entry point without pio (defaults to PIO0)
    (
        $( $module:ident { $($fields:tt)* } ),+ $(,)?
    ) => {
        define_led_strips! {
            @__with_defaults
            pio: PIO0,
            sm_counter: 0,
            strips_out: [],
            strips_in: [ $( $module { $($fields)* } ),+ ]
        }
    };

    // Process strips one at a time, adding defaults
    (@__with_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_in: [ $module:ident { $($fields:tt)* } $(, $($rest:tt)* )? ]
    ) => {
        define_led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($($rest)*)? ],
            module: $module,
            pin: __MISSING_PIN__,
            dma: DMA_CH0,
            len: __MISSING_LEN__,
            max_current: $crate::led_strip::Current::Unlimited,
            gamma: $crate::led_strip::gamma::Gamma::Linear,
            led2d: __NONE__,
            fields: [ $($fields)* ]
        }
    };

    // All strips processed, call the main implementation
    (@__with_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_in: []
    ) => {
        define_led_strips! {
            @__expand
            pio: $pio,
            strips: [ $($out)* ]
        }
    };

    // Parse fields for a single strip
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        module: $module:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: $led2d:tt,
        fields: [ pin: $new_pin:ident $(, $($rest:tt)* )? ]
    ) => {
        define_led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            module: $module,
            pin: $new_pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        module: $module:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: $led2d:tt,
        fields: [ dma: $new_dma:ident $(, $($rest:tt)* )? ]
    ) => {
        define_led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            module: $module,
            pin: $pin,
            dma: $new_dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        module: $module:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: $led2d:tt,
        fields: [ len: $new_len:expr $(, $($rest:tt)* )? ]
    ) => {
        define_led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            module: $module,
            pin: $pin,
            dma: $dma,
            len: $new_len,
            max_current: $max_current,
            gamma: $gamma,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        module: $module:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: $led2d:tt,
        fields: [ max_current: $new_max_current:expr $(, $($rest:tt)* )? ]
    ) => {
        define_led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            module: $module,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $new_max_current,
            gamma: $gamma,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        module: $module:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: $led2d:tt,
        fields: [ gamma: $new_gamma:expr $(, $($rest:tt)* )? ]
    ) => {
        define_led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            module: $module,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $new_gamma,
            led2d: $led2d,
            fields: [ $($($rest)*)? ]
        }
    };

    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        module: $module:ident,
        pin: $pin:tt,
        dma: $dma:ident,
        len: $len:tt,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: __NONE__,
        fields: [ led2d: { $($led2d_fields:tt)* } $(, $($rest:tt)* )? ]
    ) => {
        define_led_strips! {
            @__fill_strip_defaults
            pio: $pio,
            sm_counter: $sm,
            strips_out: [ $($out)* ],
            strips_remaining: [ $($remaining)* ],
            module: $module,
            pin: $pin,
            dma: $dma,
            len: $len,
            max_current: $max_current,
            gamma: $gamma,
            led2d: __HAS_LED2D__ { $($led2d_fields)* },
            fields: [ $($($rest)*)? ]
        }
    };

    // Done parsing fields, add strip to output and continue
    (@__fill_strip_defaults
        pio: $pio:ident,
        sm_counter: $sm:tt,
        strips_out: [ $($out:tt)* ],
        strips_remaining: [ $($remaining:tt)* ],
        module: $module:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: __NONE__,
        fields: []
    ) => {
        define_led_strips! {
            @__inc_counter
            pio: $pio,
            sm: $sm,
            strips_out: [
                $($out)*
                $module {
                    sm: $sm,
                    dma: $dma,
                    pin: $pin,
                    len: $len,
                    max_current: $max_current,
                    gamma: $gamma
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
        module: $module:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        len: $len:expr,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        led2d: __HAS_LED2D__ { $($led2d_fields:tt)* },
        fields: []
    ) => {
        define_led_strips! {
            @__inc_counter
            pio: $pio,
            sm: $sm,
            strips_out: [
                $($out)*
                $module {
                    sm: $sm,
                    dma: $dma,
                    pin: $pin,
                    len: $len,
                    max_current: $max_current,
                    gamma: $gamma,
                    led2d: { $($led2d_fields)* }
                },
            ],
            strips_in: [ $($remaining)* ]
        }
    };

    // Increment counter by expanding to literal numbers
    (@__inc_counter pio: $pio:ident, sm: 0, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        define_led_strips! { @__with_defaults pio: $pio, sm_counter: 1, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
    (@__inc_counter pio: $pio:ident, sm: 1, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        define_led_strips! { @__with_defaults pio: $pio, sm_counter: 2, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
    (@__inc_counter pio: $pio:ident, sm: 2, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        define_led_strips! { @__with_defaults pio: $pio, sm_counter: 3, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
    (@__inc_counter pio: $pio:ident, sm: 3, strips_out: [$($out:tt)*], strips_in: [$($in:tt)*]) => {
        define_led_strips! { @__with_defaults pio: $pio, sm_counter: 4, strips_out: [$($out)*], strips_in: [$($in)*] }
    };
}

pub use define_led_strips;

/// Used with [`define_led_strips!`] to split a PIO peripheral into 4 state machines.
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
/// use device_kit::led_strip::define_led_strips;
/// use device_kit::led_strip::Current;
/// use device_kit::led_strip::gamma::Gamma;
/// use device_kit::pio_split;
///
/// define_led_strips! {
///     Gpio2LedStrip {
///         pin: PIN_2,
///         len: 8,
///         max_current: Current::Milliamps(50),
///     }
/// }
///
/// #[embassy_executor::main]
/// async fn main(spawner: Spawner) {
///     let p = embassy_rp::init(Default::default());
///     
///     // Split PIO0 into individual state machines
///     let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO0);
///     
///     let gpio2_led_strip =
///         Gpio2LedStrip::new(sm0, p.DMA_CH0, p.PIN_2, spawner).unwrap();
/// }
///
/// ```
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
