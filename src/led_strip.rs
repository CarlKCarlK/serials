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

bind_interrupts!(struct LedStripIrqs {
    PIO1_IRQ_0 => InterruptHandler<PIO1>;
});

/// RGB color representation for the LED strip.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Rgb {
    /// Red channel (0-255)
    pub r: u8,
    /// Green channel (0-255)
    pub g: u8,
    /// Blue channel (0-255)
    pub b: u8,
}

impl Rgb {
    /// Creates a new RGB value.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

impl From<Rgb> for RGB8 {
    fn from(value: Rgb) -> Self {
        RGB8 {
            r: value.r,
            g: value.g,
            b: value.b,
        }
    }
}

impl From<RGB8> for Rgb {
    fn from(value: RGB8) -> Self {
        Self::new(value.r, value.g, value.b)
    }
}

#[derive(Clone, Copy)]
struct Frame(pub [Rgb; LED_STRIP_LEN]);

impl Default for Frame {
    fn default() -> Self {
        Self([Rgb::default(); LED_STRIP_LEN])
    }
}

/// Commands sent to the LED strip task.
enum LedStripCommand {
    Show(Frame),
}

type LedStripCommands =
    EmbassyChannel<CriticalSectionRawMutex, LedStripCommand, 4>;

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
    frame: Frame,
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
        pio1: Peri<'static, PIO1>,
        dma_ch1: Peri<'static, DMA_CH1>,
        pin: Peri<'static, PIN_2>,
        spawner: Spawner,
    ) -> Result<Self> {
        spawner
            .spawn(led_strip_driver(pio1, dma_ch1, pin, notifier.commands()))
            .map_err(Error::TaskSpawn)?;

        Ok(Self {
            commands: notifier.commands(),
            frame: Frame::default(),
        })
    }

    /// Returns the color of the LED at `index`.
    pub fn pixel(&self, index: usize) -> Result<Rgb> {
        if index >= LED_STRIP_LEN {
            return Err(Error::IndexOutOfBounds);
        }
        Ok(self.frame.0[index])
    }

    /// Updates a single LED and immediately pushes the change.
    pub async fn set_pixel(&mut self, index: usize, color: Rgb) -> Result<()> {
        if index >= LED_STRIP_LEN {
            return Err(Error::IndexOutOfBounds);
        }

        self.frame.0[index] = color;
        self.commit().await
    }

    /// Backwards-compatible alias for `update_pixel`.
    pub async fn set(&mut self, index: usize, color: Rgb) -> Result<()> {
        self.set_pixel(index, color).await
    }

    async fn commit(&self) -> Result<()> {
        let frame = self.frame;
        self.commands.send(LedStripCommand::Show(frame)).await;
        Ok(())
    }
}

impl core::ops::Index<usize> for LedStrip {
    type Output = Rgb;

    fn index(&self, index: usize) -> &Self::Output {
        &self.frame.0[index]
    }
}

#[embassy_executor::task]
async fn led_strip_driver(
    pio1: Peri<'static, PIO1>,
    dma_ch1: Peri<'static, DMA_CH1>,
    pin: Peri<'static, PIN_2>,
    commands: &'static LedStripCommands,
) -> ! {
    let mut pio = Pio::new(pio1, LedStripIrqs);
    let program = PioWs2812Program::new(&mut pio.common);
    let mut driver = PioWs2812::<PIO1, 0, LED_STRIP_LEN>::new(&mut pio.common, pio.sm0, dma_ch1, pin, &program);
    let mut rgb_buffer = [RGB8::default(); LED_STRIP_LEN];

    loop {
        let cmd = commands.receive().await;
        match cmd {
            LedStripCommand::Show(frame) => {
                for (dst, src) in rgb_buffer.iter_mut().zip(frame.0.iter()) {
                    *dst = (*src).into();
                }
                driver.write(&rgb_buffer).await;
            }
        }
    }
}
