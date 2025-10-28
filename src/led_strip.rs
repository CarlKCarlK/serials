//! Virtual LED strip driver for WS2812-style chains (PIO-based).

use embassy_rp::pio::Instance;
use embassy_rp::pio_programs::ws2812::PioWs2812;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use smart_leds::RGB8;

use crate::{Error, Result};

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

pub type LedStripCommands = EmbassyChannel<CriticalSectionRawMutex, LedStripCommand, 8>;

#[derive(Clone, Copy)]
pub enum LedStripCommand {
    Update { index: u16, color: Rgb },
}

/// Notifier used to construct LED strip instances.
pub struct LedStripNotifier {
    commands: LedStripCommands,
}

impl LedStripNotifier {
    /// Creates notifier resources.
    #[must_use]
    pub const fn notifier() -> Self {
        Self {
            commands: LedStripCommands::new(),
        }
    }

    pub fn commands(&'static self) -> &'static LedStripCommands {
        &self.commands
    }
}

/// Handle used to control a LED strip.
pub struct LedStripN<const N: usize> {
    commands: &'static LedStripCommands,
    pixels: [Rgb; N],
}

impl<const N: usize> LedStripN<N> {
    /// Creates LED strip resources.
    #[must_use]
    pub const fn notifier() -> LedStripNotifier {
        LedStripNotifier::notifier()
    }

    /// Creates a new LED strip controller bound to the given notifier.
    pub fn new(notifier: &'static LedStripNotifier) -> Result<Self> {
        Ok(Self {
            commands: notifier.commands(),
            pixels: [Rgb::default(); N],
        })
    }

    /// Returns the current color at `index`.
    pub fn pixel(&self, index: usize) -> Result<Rgb> {
        if index >= N {
            return Err(Error::IndexOutOfBounds);
        }
        Ok(self.pixels[index])
    }

    /// Updates a single LED and immediately pushes the change.
    pub async fn update_pixel(&mut self, index: usize, color: Rgb) -> Result<()> {
        if index >= N {
            return Err(Error::IndexOutOfBounds);
        }

        self.pixels[index] = color;
        self.commands
            .send(LedStripCommand::Update {
                index: index as u16,
                color,
            })
            .await;
        Ok(())
    }
}

/// Convenience alias to access `LedStripN` with a const length parameter.
pub type LedStrip<const N: usize> = LedStripN<N>;

pub async fn led_strip_driver_loop<PIO, const SM: usize, const N: usize>(
    mut driver: PioWs2812<'static, PIO, SM, N>,
    commands: &'static LedStripCommands,
) -> !
where
    PIO: Instance,
{
    let mut frame = [Rgb::default(); N];

    loop {
        match commands.receive().await {
            LedStripCommand::Update { index, color } => {
                let idx = usize::from(index);
                if idx < N {
                    frame[idx] = color;
                    driver.write(&frame).await;
                }
            }
        }
    }
}

/// Driver loop with brightness scaling.
/// Scales all RGB values by `max_brightness / 255` before writing to LEDs.
pub async fn led_strip_driver_loop_with_brightness<PIO, const SM: usize, const N: usize>(
    mut driver: PioWs2812<'static, PIO, SM, N>,
    commands: &'static LedStripCommands,
    max_brightness: u8,
) -> !
where
    PIO: Instance,
{
    let mut frame = [Rgb::default(); N];

    loop {
        match commands.receive().await {
            LedStripCommand::Update { index, color } => {
                let idx = usize::from(index);
                if idx < N {
                    // Scale the color by max_brightness
                    let scaled = Rgb::new(
                        scale_brightness(color.r, max_brightness),
                        scale_brightness(color.g, max_brightness),
                        scale_brightness(color.b, max_brightness),
                    );
                    frame[idx] = scaled;
                    driver.write(&frame).await;
                }
            }
        }
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
            /// The IRQ line that matches the selected PIO (PIOx_IRQ_y).
            irq: $irq:ident,
            /// Which state machine to use (field on `embassy_rp::pio::Pio` + index 0-3).
            sm: { field: $sm_field:ident, index: $sm_index:expr },
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
                pub type Notifier = $crate::led_strip::LedStripNotifier;

                // Calculate max brightness from current budget
                // Each WS2812B LED draws ~60mA at full brightness
                const WORST_CASE_MA: u32 = (LEN as u32) * 60;
                pub const MAX_BRIGHTNESS: u8 = {
                    let scale = ($max_current as u32 * 255) / WORST_CASE_MA;
                    if scale > 255 { 255 } else { scale as u8 }
                };

                embassy_rp::bind_interrupts!(struct Irqs {
                    $irq => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::$pio>;
                });

                #[embassy_executor::task]
                async fn $task(
                    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::$pio>,
                    dma: embassy_rp::Peri<'static, embassy_rp::peripherals::$dma>,
                    pin: embassy_rp::Peri<'static, embassy_rp::peripherals::$pin>,
                    commands: &'static $crate::led_strip::LedStripCommands,
                ) -> ! {
                    let mut pio = embassy_rp::pio::Pio::new(pio, Irqs);
                    let program = embassy_rp::pio_programs::ws2812::PioWs2812Program::new(&mut pio.common);
                    let driver = embassy_rp::pio_programs::ws2812::PioWs2812::<embassy_rp::peripherals::$pio, $sm_index, LEN>::new(
                        &mut pio.common,
                        pio.$sm_field,
                        dma,
                        pin,
                        &program,
                    );
$crate::led_strip::led_strip_driver_loop_with_brightness::<embassy_rp::peripherals::$pio, $sm_index, LEN>(driver, commands, MAX_BRIGHTNESS).await;
                }

                pub const fn notifier() -> Notifier {
                    Strip::notifier()
                }

                pub fn new(
                    spawner: Spawner,
                    notifier: &'static Notifier,
                    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::$pio>,
                    dma: embassy_rp::Peri<'static, embassy_rp::peripherals::$dma>,
                    pin: embassy_rp::Peri<'static, embassy_rp::peripherals::$pin>,
                ) -> $crate::Result<Strip> {
                    spawner.spawn($task(pio, dma, pin, notifier.commands()).unwrap());
                    Strip::new(notifier)
                }
            }
            $(pub use $module as $alias;)?
        )+
    };
}
