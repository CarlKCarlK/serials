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
pub struct GenericLedStrip<const N: usize> {
    commands: &'static LedStripCommands,
    pixels: [Rgb; N],
}

impl<const N: usize> GenericLedStrip<N> {
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

/// Convenience alias to access `GenericLedStrip` with a const length parameter.
pub type LedStrip<const N: usize> = GenericLedStrip<N>;

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

#[macro_export]
macro_rules! define_led_strip_targets {
    ($(
        $task:ident : {
            pio: $pio:ident,
            irqs: $irqs:ident,
            sm: { field: $sm_field:ident, index: $sm_index:expr },
            dma: $dma:ident,
            pin: $pin:ident,
            len: $len:expr
        }
    ),+ $(,)?) => {
        $(
            #[embassy_executor::task]
            pub async fn $task(
                pio: embassy_rp::Peri<'static, embassy_rp::peripherals::$pio>,
                dma: embassy_rp::Peri<'static, embassy_rp::peripherals::$dma>,
                pin: embassy_rp::Peri<'static, embassy_rp::peripherals::$pin>,
                commands: &'static $crate::led_strip::LedStripCommands,
            ) -> ! {
                let mut pio = embassy_rp::pio::Pio::new(pio, $irqs);
                let program = embassy_rp::pio_programs::ws2812::PioWs2812Program::new(&mut pio.common);
                let driver = embassy_rp::pio_programs::ws2812::PioWs2812::<embassy_rp::peripherals::$pio, $sm_index, $len>::new(
                    &mut pio.common,
                    pio.$sm_field,
                    dma,
                    pin,
                    &program,
                );
                $crate::led_strip::led_strip_driver_loop::<embassy_rp::peripherals::$pio, $sm_index, $len>(driver, commands).await;
            }
        )+
    };
}
