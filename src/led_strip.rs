//! Virtual LED strip device driven by a PIO-based WS2812 driver.

use embassy_executor::Spawner;
use embassy_rp::{Peri, bind_interrupts};
use embassy_rp::peripherals::{DMA_CH1, PIN_2, PIO1};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use smart_leds::RGB8;

use crate::{Error, Result};

pub const LED_STRIP_LEN: usize = 8;
pub type LedStripDataPin = PIN_2;

/// Re-export the RGB color representation used by the driver.
pub type Rgb = RGB8;

bind_interrupts!(struct LedStripIrqs {
    PIO1_IRQ_0 => InterruptHandler<PIO1>;
});

/// Commands sent to the LED strip task.
enum LedStripCommand {
    Update { index: u8, color: Rgb },
}

type LedStripCommands = EmbassyChannel<CriticalSectionRawMutex, LedStripCommand, 8>;

/// Notifier used to construct LED strip instances.
pub struct LedStripNotifier {
    commands: LedStripCommands,
}

impl LedStripNotifier {
    /// Creates the notifier resources.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: EmbassyChannel::new(),
        }
    }

    fn commands(&'static self) -> &'static LedStripCommands {
        &self.commands
    }
}

/// Virtual LED strip device handle.
pub struct LedStrip {
    commands: &'static LedStripCommands,
    pixels: [Rgb; LED_STRIP_LEN],
}

impl LedStrip {
    /// Creates LED strip resources.
    #[must_use]
    pub const fn notifier() -> LedStripNotifier {
        LedStripNotifier::new()
    }

    /// Creates a new LED strip controller bound to the given notifier.
    pub fn new(
        notifier: &'static LedStripNotifier,
        pio: Peri<'static, PIO1>,
        dma: Peri<'static, DMA_CH1>,
        pin: Peri<'static, LedStripDataPin>,
        spawner: Spawner,
    ) -> Result<Self> {
        spawner
            .spawn(led_strip_driver(pio, dma, pin, notifier.commands()))
            .map_err(Error::TaskSpawn)?;

        Ok(Self {
            commands: notifier.commands(),
            pixels: [Rgb::default(); LED_STRIP_LEN],
        })
    }

    /// Returns the color of the LED at index.
    pub fn pixel(&self, index: usize) -> Result<Rgb> {
        if index >= LED_STRIP_LEN {
            return Err(Error::IndexOutOfBounds);
        }
        Ok(self.pixels[index])
    }

    /// Updates a single LED and immediately pushes the change.
    pub async fn update_pixel(&mut self, index: usize, color: Rgb) -> Result<()> {
        if index >= LED_STRIP_LEN {
            return Err(Error::IndexOutOfBounds);
        }

        self.pixels[index] = color;
        self.commands
            .send(LedStripCommand::Update {
                index: index as u8,
                color,
            })
            .await;
        Ok(())
    }
}

#[embassy_executor::task]
async fn led_strip_driver(
    pio: Peri<'static, PIO1>,
    dma: Peri<'static, DMA_CH1>,
    pin: Peri<'static, LedStripDataPin>,
    commands: &'static LedStripCommands,
) -> ! {
    let mut pio = Pio::new(pio, LedStripIrqs);
    let program = PioWs2812Program::new(&mut pio.common);
    let mut driver = PioWs2812::<PIO1, 0, LED_STRIP_LEN>::new(&mut pio.common, pio.sm0, dma, pin, &program);
    let mut frame = [Rgb::default(); LED_STRIP_LEN];

    loop {
        match commands.receive().await {
            LedStripCommand::Update { index, color } => {
                let idx = usize::from(index);
                frame[idx] = color;
                driver.write(&frame).await;
            }
        }
    }
}
