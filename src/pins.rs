use core::convert::Infallible;

use embassy_rp::{
    gpio::{self, Level},
    peripherals::CORE1,
};
use embedded_hal::digital::OutputPin;

use crate::shared_constants::{CELL_COUNT0, SEGMENT_COUNT0};

pub struct OutputArray<'a, const N: usize>([gpio::Output<'a>; N]);

impl<'a, const N: usize> OutputArray<'a, N> {
    pub fn new(outputs: [gpio::Output<'a>; N]) -> Self {
        Self(outputs)
    }

    #[inline]
    pub fn set_levels_at_indexes(&mut self, indexes: &[usize], level: Level) {
        for &index in indexes {
            self.0[index].set_level(level);
        }
    }
}

impl OutputArray<'_, { u8::BITS as usize }> {
    #[inline]
    #[must_use = "Possible error result should not be ignored"]
    // on some hardware (but not here), setting a bit can fail, so we return a Result
    pub fn set_from_bits(&mut self, mut bits: u8) -> Result<(), Infallible> {
        for output in &mut self.0 {
            let state = (bits & 1) == 1;
            output.set_state(state.into())?;
            bits >>= 1;
        }
        Ok(())
    }
}

#[allow(dead_code)] // We aren't using led0 in this example
pub struct Pins {
    pub cells0: OutputArray<'static, CELL_COUNT0>,
    pub segments0: OutputArray<'static, SEGMENT_COUNT0>,
    pub button: gpio::Input<'static>,
    led0: gpio::Output<'static>,
}

impl Pins {
    pub fn new_and_core1() -> (Self, CORE1) {
        let peripherals: embassy_rp::Peripherals =
            embassy_rp::init(embassy_rp::config::Config::default());
        let core1 = peripherals.CORE1;

        let cells0 = OutputArray::new([
            gpio::Output::new(peripherals.PIN_1, Level::High),
            gpio::Output::new(peripherals.PIN_2, Level::High),
            gpio::Output::new(peripherals.PIN_3, Level::High),
            gpio::Output::new(peripherals.PIN_4, Level::High),
        ]);

        let segments0 = OutputArray::new([
            gpio::Output::new(peripherals.PIN_5, Level::Low),
            gpio::Output::new(peripherals.PIN_6, Level::Low),
            gpio::Output::new(peripherals.PIN_7, Level::Low),
            gpio::Output::new(peripherals.PIN_8, Level::Low),
            gpio::Output::new(peripherals.PIN_9, Level::Low),
            gpio::Output::new(peripherals.PIN_10, Level::Low),
            gpio::Output::new(peripherals.PIN_11, Level::Low),
            gpio::Output::new(peripherals.PIN_12, Level::Low),
        ]);

        let button = gpio::Input::new(peripherals.PIN_13, gpio::Pull::Down);

        let led0 = gpio::Output::new(peripherals.PIN_0, Level::Low);

        (
            Self {
                cells0,
                segments0,
                button,
                led0,
            },
            core1,
        )
    }
}
