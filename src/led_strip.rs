//! Virtual LED strip driver for WS2812-style chains (PIO-based).

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::dma::Channel;
use embassy_rp::pio::{Instance, InterruptHandler, Pio};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::peripherals;
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use smart_leds::RGB8;

use crate::{Error, Result};

bind_interrupts!(struct Pio1Irqs {
    PIO1_IRQ_0 => InterruptHandler<peripherals::PIO1>;
});

/// Default LED strip length used throughout the examples.
pub const LED_STRIP_LEN: usize = 8;

/// RGB color representation re-exported from `smart_leds`.
pub type Rgb = RGB8;

pub(crate) type LedStripCommands = EmbassyChannel<CriticalSectionRawMutex, LedStripCommand, 8>;

pub(crate) trait LedStripPin<const N: usize, PIO: Instance, DMA: Channel>: Sized {
    fn spawn_driver(
        self,
        spawner: Spawner,
        pio: Peri<'static, PIO>,
        dma: Peri<'static, DMA>,
        commands: &'static LedStripCommands,
    ) -> Result<()>;
}

#[derive(Clone, Copy)]
pub(crate) enum LedStripCommand {
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

    fn commands(&'static self) -> &'static LedStripCommands {
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
    #[allow(private_bounds)]
    pub fn new<PIN, PIO, DMA>(
        notifier: &'static LedStripNotifier,
        pio: Peri<'static, PIO>,
        dma: Peri<'static, DMA>,
        pin: PIN,
        spawner: Spawner,
    ) -> Result<Self>
    where
        PIO: Instance,
        DMA: Channel,
        PIN: LedStripPin<N, PIO, DMA>,
    {
        pin.spawn_driver(spawner, pio, dma, notifier.commands())?;

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

/// Backwards-compatible alias for the default LED strip length.
pub type LedStrip = GenericLedStrip<LED_STRIP_LEN>;

async fn led_strip_driver_loop<PIO, const SM: usize, const N: usize>(
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
            async fn $task(
                pio: Peri<'static, peripherals::$pio>,
                dma: Peri<'static, peripherals::$dma>,
                pin: Peri<'static, peripherals::$pin>,
                commands: &'static LedStripCommands,
            ) -> ! {
                let mut pio = Pio::new(pio, $irqs);
                let program = PioWs2812Program::new(&mut pio.common);
                let driver = PioWs2812::<peripherals::$pio, $sm_index, $len>::new(
                    &mut pio.common,
                    pio.$sm_field,
                    dma,
                    pin,
                    &program,
                );
                led_strip_driver_loop::<peripherals::$pio, $sm_index, $len>(driver, commands).await;
            }

            impl LedStripPin<$len, peripherals::$pio, peripherals::$dma> for Peri<'static, peripherals::$pin> {
                fn spawn_driver(
                    self,
                    spawner: Spawner,
                    pio: Peri<'static, peripherals::$pio>,
                    dma: Peri<'static, peripherals::$dma>,
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

define_led_strip_targets! {
    led_strip_driver_pio1_sm0_pin2_len_default: {
        pio: PIO1,
        irqs: Pio1Irqs,
        sm: { field: sm0, index: 0 },
        dma: DMA_CH1,
        pin: PIN_2,
        len: LED_STRIP_LEN
    }
}
