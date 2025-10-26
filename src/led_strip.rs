//! Virtual LED strip driver for WS2812-style chains (PIO-based).

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::pio::{InterruptHandler, Pio, PioPin};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::peripherals::{self, DMA_CH1, PIO1};
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use smart_leds::RGB8;

use crate::{Error, Result};

bind_interrupts!(struct Pio1Irqs {
    PIO1_IRQ_0 => InterruptHandler<PIO1>;
});

/// Default LED strip length used throughout the examples.
pub const LED_STRIP_LEN: usize = 8;

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

pub(crate) type LedStripCommands = EmbassyChannel<CriticalSectionRawMutex, LedStripCommand, 8>;

pub(crate) trait LedStripPin: Sized {
    fn spawn_driver(
        self,
        spawner: Spawner,
        pio: Peri<'static, PIO1>,
        dma: Peri<'static, DMA_CH1>,
        commands: &'static LedStripCommands,
    ) -> Result<()>;
}

#[derive(Clone, Copy)]
pub(crate) enum LedStripCommand {
    Update { index: u8, color: Rgb },
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

    fn commands(&'static self) -> &'static LedStripCommands {
        &self.commands
    }
}

/// Handle used to control a LED strip.
pub struct LedStrip {
    commands: &'static LedStripCommands,
    pixels: [Rgb; LED_STRIP_LEN],
}

impl LedStrip {
    /// Creates LED strip resources.
    #[must_use]
    pub const fn notifier() -> LedStripNotifier {
        LedStripNotifier::notifier()
    }

    /// Creates a new LED strip controller bound to the given notifier.
    #[allow(private_bounds)]
    pub fn new(
        notifier: &'static LedStripNotifier,
        pio: Peri<'static, PIO1>,
        dma: Peri<'static, DMA_CH1>,
        pin: impl LedStripPin,
        spawner: Spawner,
    ) -> Result<Self> {
        pin.spawn_driver(spawner, pio, dma, notifier.commands())?;

        Ok(Self {
            commands: notifier.commands(),
            pixels: [Rgb::default(); LED_STRIP_LEN],
        })
    }

    /// Returns the current color at `index`.
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

async fn led_strip_driver_inner<PIN>(
    pio_peripheral: Peri<'static, PIO1>,
    dma: Peri<'static, DMA_CH1>,
    pin: Peri<'static, PIN>,
    commands: &'static LedStripCommands,
) -> !
where
    PIN: PioPin + 'static,
{
    let mut pio = Pio::new(pio_peripheral, Pio1Irqs);
    let program = PioWs2812Program::new(&mut pio.common);
    let mut driver = PioWs2812::<PIO1, 0, LED_STRIP_LEN>::new(&mut pio.common, pio.sm0, dma, pin, &program);
    let mut frame = [Rgb::default(); LED_STRIP_LEN];

    loop {
        match commands.receive().await {
            LedStripCommand::Update { index, color } => {
                let idx = usize::from(index);
                if idx < LED_STRIP_LEN {
                    frame[idx] = color;
                    driver.write(&frame).await;
                }
            }
        }
    }
}

macro_rules! impl_led_strip_pin {
    ($(($pin:ident, $task:ident)),+ $(,)?) => {
        $(
            #[embassy_executor::task]
            async fn $task(
                pio_peripheral: Peri<'static, PIO1>,
                dma: Peri<'static, DMA_CH1>,
                pin: Peri<'static, peripherals::$pin>,
                commands: &'static LedStripCommands,
            ) -> ! {
                led_strip_driver_inner(pio_peripheral, dma, pin, commands).await
            }

            impl LedStripPin for Peri<'static, peripherals::$pin> {
                fn spawn_driver(
                    self,
                    spawner: Spawner,
                    pio: Peri<'static, PIO1>,
                    dma: Peri<'static, DMA_CH1>,
                    commands: &'static LedStripCommands,
                ) -> Result<()> {
                    spawner
                        .spawn($task(pio, dma, self, commands))
                        .map_err(Error::TaskSpawn)
                }
            }
        )+
    };
}

impl_led_strip_pin!(
    (PIN_0, led_strip_driver_pin_0),
    (PIN_1, led_strip_driver_pin_1),
    (PIN_2, led_strip_driver_pin_2),
    (PIN_3, led_strip_driver_pin_3),
    (PIN_4, led_strip_driver_pin_4),
    (PIN_5, led_strip_driver_pin_5),
    (PIN_6, led_strip_driver_pin_6),
    (PIN_7, led_strip_driver_pin_7),
    (PIN_8, led_strip_driver_pin_8),
    (PIN_9, led_strip_driver_pin_9),
    (PIN_10, led_strip_driver_pin_10),
    (PIN_11, led_strip_driver_pin_11),
    (PIN_12, led_strip_driver_pin_12),
    (PIN_13, led_strip_driver_pin_13),
    (PIN_14, led_strip_driver_pin_14),
    (PIN_15, led_strip_driver_pin_15),
    (PIN_16, led_strip_driver_pin_16),
    (PIN_17, led_strip_driver_pin_17),
    (PIN_18, led_strip_driver_pin_18),
    (PIN_19, led_strip_driver_pin_19),
    (PIN_20, led_strip_driver_pin_20),
    (PIN_21, led_strip_driver_pin_21),
    (PIN_22, led_strip_driver_pin_22),
    (PIN_23, led_strip_driver_pin_23),
    (PIN_24, led_strip_driver_pin_24),
    (PIN_25, led_strip_driver_pin_25),
    (PIN_26, led_strip_driver_pin_26),
    (PIN_27, led_strip_driver_pin_27),
    (PIN_28, led_strip_driver_pin_28),
    (PIN_29, led_strip_driver_pin_29),
);
