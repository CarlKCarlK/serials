//! A device abstraction for WS2812-style LED strips.
//!
//! See [`LedStrip`] for the main usage example.

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

/// Maximum supported LED strip length
pub const MAX_LEDS: usize = 256;

// ============================================================================
// PIO Bus - Shared PIO resource for multiple LED strips
// ============================================================================

/// Shared PIO bus that manages the Common resource and WS2812 program
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
            self.common.lock(|common_cell| {
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
        self.common.lock(|common_cell| {
            let mut common = common_cell.borrow_mut();
            f(&mut *common)
        })
    }
}

// ============================================================================
// LED Strip Command Channel and Static
// ============================================================================

pub type LedStripCommands<const N: usize> = EmbassyChannel<CriticalSectionRawMutex, [Rgb; N], 2>;

/// Static used to construct LED strip instances.
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

/// A device abstraction for WS2812-style LED strips with configurable length.
///
/// ```no_run
/// # #![no_std]
/// # use panic_probe as _;
/// # fn main() {}
/// use device_kit::led_strip::{LedStrip, LedStripStatic, Rgb};
///
/// async fn example() -> device_kit::Result<()> {
///     static LED_STRIP_STATIC: LedStripStatic<8> = LedStrip::new_static();
///     let mut strip: LedStrip<8> = LedStrip::new(&LED_STRIP_STATIC)?;
///
///     let red = Rgb::new(16, 0, 0);
///     let green = Rgb::new(0, 16, 0);
///     let blue = Rgb::new(0, 0, 16);
///     let frame = [
///         red, green, blue, red, green, blue, red, green,
///     ];
///     strip.update_pixels(&frame).await?;
///     Ok(())
/// }
/// ```
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
    pub async fn update_pixels(&mut self, pixels: &[Rgb; N]) -> Result<()> {
        // Send entire frame as one message (copy array to send through channel)
        self.commands.send(*pixels).await;

        // Wait for the DMA write to complete
        embassy_time::Timer::after(embassy_time::Duration::from_micros(Self::WRITE_DELAY_US)).await;

        Ok(())
    }
}

impl<const N: usize> crate::led2d::LedStrip<N> for LedStrip<N> {
    async fn update_pixels(&mut self, pixels: &[Rgb; N]) -> Result<()> {
        self.update_pixels(pixels).await
    }
}

// cmk likely shouldn't be pub
/// Driver loop with brightness scaling.
/// Scales all RGB values by `max_brightness / 255` before writing to LEDs.
pub async fn led_strip_driver_loop<PIO, const SM: usize, const N: usize, ORDER>(
    mut driver: PioWs2812<'static, PIO, SM, N, ORDER>,
    commands: &'static LedStripCommands<N>,
    max_brightness: u8,
) -> !
where
    PIO: Instance,
    ORDER: embassy_rp::pio_programs::ws2812::RgbColorOrder,
{
    loop {
        let mut frame = commands.receive().await;

        // Scale all pixels by brightness in place
        for color in frame.iter_mut() {
            *color = Rgb::new(
                scale_brightness(color.r, max_brightness),
                scale_brightness(color.g, max_brightness),
                scale_brightness(color.b, max_brightness),
            );
        }

        driver.write(&frame).await;
    }
}

/// Scale a single color component by brightness (0-255).
#[inline]
fn scale_brightness(value: u8, brightness: u8) -> u8 {
    ((u16::from(value) * u16::from(brightness)) / 255) as u8
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
/// # Example
/// ```no_run
/// #![no_std]
/// use panic_probe as _;
/// // Requires target support and macro imports; no_run to avoid hardware access in doctests.
/// # fn main() {}
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! define_led_strips {
    (
        pio: $pio:ident,
        strips: [
            $(
                $module:ident {
                    sm: $sm_index:expr,
                    dma: $dma:ident,
                    pin: $pin:ident,
                    len: $len:expr,
                    max_current: $max_current:expr
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

            // Helper function to split the PIO into bus and state machines
            // Returns (bus, sm0, sm1, sm2, sm3)
            pub fn [<$pio:lower _split>](
                pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
            ) -> (
                &'static $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>,
                ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, 0>,
                ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, 1>,
                ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, 2>,
                ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, 3>,
            ) {
                let ::embassy_rp::pio::Pio { common, sm0, sm1, sm2, sm3, .. } =
                    ::embassy_rp::pio::Pio::new(pio, $crate::pio_irqs::[<$pio:camel Irqs>]);
                let pio_bus = [<$pio _BUS>].init_with(|| {
                    $crate::led_strip::PioBus::new(common)
                });
                (pio_bus, sm0, sm1, sm2, sm3)
            }
        }

        // Create strip modules
        $(
            #[allow(non_snake_case)]
            pub mod $module {
                use super::*;
                use ::embassy_executor::Spawner;

                pub const LEN: usize = $len;
                pub type Strip = $crate::led_strip::LedStrip<LEN>;
                pub type Static = $crate::led_strip::LedStripStatic<LEN>;

                // Calculate max brightness from current budget
                // Each WS2812B LED draws ~60mA at full brightness
                const WORST_CASE_MA: u32 = (LEN as u32) * 60;
                pub const MAX_BRIGHTNESS: u8 = {
                    let scale = ($max_current.as_u32() * 255) / WORST_CASE_MA;
                    if scale > 255 { 255 } else { scale as u8 }
                };

                paste::paste! {
                    #[::embassy_executor::task]
                    async fn [<$module _driver>](
                        bus: &'static $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>,
                        sm: ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, $sm_index>,
                        dma: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>,
                        pin: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>,
                        commands: &'static $crate::led_strip::LedStripCommands<LEN>,
                    ) -> ! {
                        let program = bus.get_program();
                        let driver = bus.with_common(|common| {
                            ::embassy_rp::pio_programs::ws2812::PioWs2812::<
                                ::embassy_rp::peripherals::$pio,
                                $sm_index,
                                LEN,
                                _
                            >::new(common, sm, dma, pin, program)
                        });
                        $crate::led_strip::led_strip_driver_loop::<
                            ::embassy_rp::peripherals::$pio,
                            $sm_index,
                            LEN,
                            _
                        >(driver, commands, MAX_BRIGHTNESS).await
                    }
                }

                pub const fn new_static() -> Static {
                    Strip::new_static()
                }

                paste::paste! {
                    pub fn new(
                        spawner: Spawner,
                        strip_static: &'static Static,
                        bus: &'static $crate::led_strip::PioBus<'static, ::embassy_rp::peripherals::$pio>,
                        sm: ::embassy_rp::pio::StateMachine<'static, ::embassy_rp::peripherals::$pio, $sm_index>,
                        dma: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>,
                        pin: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>,
                    ) -> $crate::Result<Strip> {
                        let token = [<$module _driver>](bus, sm, dma, pin, strip_static.commands()).map_err($crate::Error::TaskSpawn)?;
                        spawner.spawn(token);
                        Strip::new(strip_static)
                    }
                }
            }
        )+
    };
}

pub use crate::define_led_strips;

/// Predefined RGB color constants (RED, GREEN, BLUE, etc.).
pub use smart_leds::colors;
