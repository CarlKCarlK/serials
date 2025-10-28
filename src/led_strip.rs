//! Virtual LED strip driver for WS2812-style chains (PIO-based).

use embassy_rp::pio::Instance;
use embassy_rp::pio_programs::ws2812::PioWs2812;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use smart_leds::RGB8;

use crate::Result;

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

/// Maximum supported LED strip length
pub const MAX_LEDS: usize = 256;

pub type LedStripCommands<const N: usize> = EmbassyChannel<CriticalSectionRawMutex, [Rgb; N], 2>;

/// Notifier used to construct LED strip instances.
pub struct LedStripNotifier<const N: usize> {
    commands: LedStripCommands<N>,
}

impl<const N: usize> LedStripNotifier<N> {
    /// Creates notifier resources.
    #[must_use]
    pub const fn notifier() -> Self {
        Self {
            commands: LedStripCommands::new(),
        }
    }

    pub fn commands(&'static self) -> &'static LedStripCommands<N> {
        &self.commands
    }
}

/// Handle used to control a LED strip.
pub struct LedStripN<const N: usize> {
    commands: &'static LedStripCommands<N>,
}

impl<const N: usize> LedStripN<N> {
    /// WS2812B timing: ~30µs per LED + 100µs safety margin
    const WRITE_DELAY_US: u64 = (N as u64 * 30) + 100;

    /// Creates LED strip resources.
    #[must_use]
    pub const fn notifier() -> LedStripNotifier<N> {
        LedStripNotifier::notifier()
    }

    /// Creates a new LED strip controller bound to the given notifier.
    pub fn new(notifier: &'static LedStripNotifier<N>) -> Result<Self> {
        Ok(Self {
            commands: notifier.commands(),
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
pub async fn led_strip_driver_loop<PIO, const SM: usize, const N: usize>(
    mut driver: PioWs2812<'static, PIO, SM, N>,
    commands: &'static LedStripCommands<N>,
    max_brightness: u8,
) -> !
where
    PIO: Instance,
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

#[macro_export]
macro_rules! define_led_strip {
    ($(
        $module:ident $(as $alias:ident)? {
            $(#[$meta:meta])*
            task: $task:ident,
            /// Which PIO peripheral owns this strip (PIO0/PIO1).
            pio: $pio:ident,
            /// Which state machine to use (0-3).
            sm_index: $sm_index:expr,
            /// DMA channel feeding the PIO TX FIFO.
            dma: $dma:ident,
            /// GPIO pin that carries the strip's data signal.
            pin: $pin:ident,
            /// Number of LEDs on the strip.
            len: $len:expr,
            /// Maximum current budget in milliamps (mA).
            max_current_ma: $max_current:expr
        }
    ),+ $(,)?) => {
        $(
            #[allow(non_snake_case)]
            #[allow(non_snake_case)]
            pub mod $module {
                use super::*;
                use embassy_executor::Spawner;

                pub const LEN: usize = $len;
                pub type Strip = $crate::led_strip::LedStrip<LEN>;
                pub type Notifier = $crate::led_strip::LedStripNotifier<LEN>;

                // Calculate max brightness from current budget
                // Each WS2812B LED draws ~60mA at full brightness
                const WORST_CASE_MA: u32 = (LEN as u32) * 60;
                pub const MAX_BRIGHTNESS: u8 = {
                    let scale = ($max_current as u32 * 255) / WORST_CASE_MA;
                    if scale > 255 { 255 } else { scale as u8 }
                };

                #[embassy_executor::task]
                async fn $task(
                    common: *mut embassy_rp::pio::Common<'static, embassy_rp::peripherals::$pio>,
                    sm: embassy_rp::pio::StateMachine<'static, embassy_rp::peripherals::$pio, $sm_index>,
                    dma: embassy_rp::Peri<'static, embassy_rp::peripherals::$dma>,
                    pin: embassy_rp::Peri<'static, embassy_rp::peripherals::$pin>,
                    commands: &'static $crate::led_strip::LedStripCommands<LEN>,
                ) -> ! {
                    let common = unsafe { &mut *common };
                    let program = embassy_rp::pio_programs::ws2812::PioWs2812Program::new(common);
                    let driver = embassy_rp::pio_programs::ws2812::PioWs2812::<embassy_rp::peripherals::$pio, $sm_index, LEN>::new(
                        common,
                        sm,
                        dma,
                        pin,
                        &program,
                    );
$crate::led_strip::led_strip_driver_loop::<embassy_rp::peripherals::$pio, $sm_index, LEN>(driver, commands, MAX_BRIGHTNESS).await;
                }

                pub const fn notifier() -> Notifier {
                    Strip::notifier()
                }

                pub fn new(
                    spawner: Spawner,
                    notifier: &'static Notifier,
                    common: &'static mut embassy_rp::pio::Common<'static, embassy_rp::peripherals::$pio>,
                    sm: embassy_rp::pio::StateMachine<'static, embassy_rp::peripherals::$pio, $sm_index>,
                    dma: embassy_rp::Peri<'static, embassy_rp::peripherals::$dma>,
                    pin: embassy_rp::Peri<'static, embassy_rp::peripherals::$pin>,
                ) -> $crate::Result<Strip> {
                    spawner.spawn($task(common as *mut _, sm, dma, pin, notifier.commands()).unwrap());
                    Strip::new(notifier)
                }
            }
            $(pub use $module as $alias;)?
        )+
    };
}
