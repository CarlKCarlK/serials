//! A device abstraction for WS2812-style LED strips.
//!
//! For simple single-strip setups, see the example on [`define_led_strips!`].
//! For the core device API, see [`LedStripN`].

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
/// Typically used via the [`define_led_strips!`] macro, which generates properly configured
/// modules with type aliases and constructor functions. The macro handles PIO setup,
/// interrupt bindings, and brightness limiting automatically.
///
/// For direct usage (advanced), you must manually set up the PIO bus, state machine,
/// and driver task. See the [`define_led_strips!`] macro for the recommended approach.
pub struct LedStripN<const N: usize> {
    commands: &'static LedStripCommands<N>,
}

impl<const N: usize> LedStripN<N> {
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

/// Convenience alias to access `LedStripN` with a const length parameter.
pub type LedStrip<const N: usize> = LedStripN<N>;

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
/// This macro generates all the necessary code to create WS2812-style LED strips
/// using a single PIO peripheral. It handles interrupt bindings, PIO bus sharing, and
/// per-strip brightness limiting based on current budget.
///
/// # Single Strip Example
///
/// For controlling one LED strip, define a single strip module:
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use embassy_executor::Spawner;
/// use serials::led_strip::{define_led_strips, Rgb};
///
/// define_led_strips! {
///     pio: PIO0,
///     strips: [
///         my_strip {
///             sm: 0,
///             dma: DMA_CH0,
///             pin: PIN_2,
///             len: 8,
///             max_current_ma: 480
///         }
///     ]
/// }
///
/// #[embassy_executor::main]
/// async fn main(spawner: Spawner) -> ! {
///     let peripherals = embassy_rp::init(Default::default());
///
///     // Split PIO into bus and state machines
///     let (pio_bus, sm0, _sm1, _sm2, _sm3) = pio0_split(peripherals.PIO0);
///
///     // Create and initialize the strip
///     static STRIP_STATIC: my_strip::Static = my_strip::new_static();
///     let mut strip = my_strip::new(
///         spawner,
///         &STRIP_STATIC,
///         pio_bus,
///         sm0,
///         peripherals.DMA_CH0.into(),
///         peripherals.PIN_2.into(),
///     )
///     .expect("Failed to start LED strip");
///
///     // Update pixels
///     let red = Rgb::new(16, 0, 0);
///     let frame = [red; my_strip::LEN];
///     strip.update_pixels(&frame).await.expect("update failed");
///     # loop {}
/// }
/// ```
///
/// # Parameters
///
/// * `pio` - The PIO peripheral to use (PIO0, PIO1, or PIO2 on RISC-V)
/// * `sm` - State machine index (0-3)
/// * `dma` - DMA channel peripheral name
/// * `pin` - GPIO pin peripheral name for LED data line
/// * `len` - Number of LEDs in the strip (up to [`MAX_LEDS`])
/// * `max_current_ma` - Maximum current budget in milliamps; brightness is automatically
///   scaled to stay within this limit (each LED draws ~60mA at full white)
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
                    max_current_ma: $max_current:expr
                }
            ),+ $(,)?
        ]
    ) => {
        // Generate interrupt binding struct name from PIO name (PIO0 -> Pio0Irqs, PIO1 -> Pio1Irqs)
        paste::paste! {
            ::embassy_rp::bind_interrupts!(struct [<$pio:camel Irqs>] {
                [<$pio _IRQ_0>] => ::embassy_rp::pio::InterruptHandler<::embassy_rp::peripherals::$pio>;
            });

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
                let ::embassy_rp::pio::Pio { common, sm0, sm1, sm2, sm3, .. } = ::embassy_rp::pio::Pio::new(pio, [<$pio:camel Irqs>]);
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

                // Validate SM index at compile time
                const _: () = {
                    const SM: usize = $sm_index;
                    if SM > 3 {
                        panic!("State machine index must be 0, 1, 2, or 3");
                    }
                };

                // Validate PIO/pin compatibility on RP2350 (Pico 2)
                #[cfg(feature = "pico2")]
                const _: () = {
                    const PIO_ID: u8 = $crate::led_strip::pio_id(stringify!($pio));
                    const PIN_NUM: u8 = $crate::led_strip::pin_number(stringify!($pin));
                    if !$crate::led_strip::pio_can_use_pin(PIO_ID, PIN_NUM) {
                        panic!(concat!(
                            "Pin ", stringify!($pin), " is incompatible with ",
                            stringify!($pio), " on RP2350"
                        ));
                    }
                };

                pub const LEN: usize = $len;
                pub type Strip = $crate::led_strip::LedStrip<LEN>;
                pub type Static = $crate::led_strip::LedStripStatic<LEN>;

                // Calculate max brightness from current budget
                // Each WS2812B LED draws ~60mA at full brightness
                const WORST_CASE_MA: u32 = (LEN as u32) * 60;
                pub const MAX_BRIGHTNESS: u8 = {
                    let scale = ($max_current as u32 * 255) / WORST_CASE_MA;
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

// ============================================================================
// Compile-time validation helpers
// ============================================================================

/// Extract PIO number from PIO peripheral name ("PIO0" -> 0, "PIO1" -> 1, "PIO2" -> 2).
#[doc(hidden)]
pub const fn pio_id(pio_name: &str) -> u8 {
    let bytes = pio_name.as_bytes();
    assert!(bytes.len() == 4, "PIO name must be PIO0, PIO1, or PIO2");
    assert!(bytes[0] == b'P' && bytes[1] == b'I' && bytes[2] == b'O', "Invalid PIO name");
    bytes[3] - b'0'
}

/// Extract pin number from PIN peripheral name ("PIN_2" -> 2, "PIN_16" -> 16).
#[doc(hidden)]
pub const fn pin_number(pin_name: &str) -> u8 {
    let bytes = pin_name.as_bytes();
    assert!(bytes.len() >= 5, "PIN name too short");
    assert!(
        bytes[0] == b'P' && bytes[1] == b'I' && bytes[2] == b'N' && bytes[3] == b'_',
        "PIN name must start with PIN_"
    );
    
    // Parse remaining digits
    let mut num: u8 = 0;
    let mut index = 4;
    while index < bytes.len() {
        let digit = bytes[index];
        assert!(digit >= b'0' && digit <= b'9', "PIN name must end with digits");
        num = num * 10 + (digit - b'0');
        index += 1;
    }
    num
}

/// Check if a PIO can use a specific pin on RP2350.
/// On RP2040, all pins work with all PIOs (returns true).
/// On RP2350, PIO2 has restrictions.
#[doc(hidden)]
pub const fn pio_can_use_pin(pio_id: u8, pin_num: u8) -> bool {
    #[cfg(not(feature = "pico2"))]
    {
        // RP2040: all PIOs can use all pins
        let _ = (pio_id, pin_num);
        true
    }
    
    #[cfg(feature = "pico2")]
    {
        // RP2350 PIO2 restrictions (only on RISC-V, but we check for all pico2)
        // PIO2 can only use pins 24-29 and 47
        if pio_id == 2 {
            matches!(pin_num, 24..=29 | 47)
        } else {
            // PIO0 and PIO1 can use any pin
            true
        }
    }
}
