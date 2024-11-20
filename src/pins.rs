use crate::virtual_display::{CELL_COUNT1, SEGMENT_COUNT1};
use embassy_rp::{
    gpio::{self, Level},
    peripherals::CORE1,
};
use embedded_hal::digital::OutputPin; // cmk why doesn't Brad's code need this?

pub struct OutputArray<const N: usize>([gpio::Output<'static>; N]);

impl<const N: usize> OutputArray<N> {
    pub fn new(outputs: [gpio::Output<'static>; N]) -> Self {
        Self(outputs)
    }

    #[inline]
    pub fn set_levels_at_indexes(&mut self, indexes: &[usize], level: Level) {
        for &index in indexes {
            self.0[index].set_level(level);
        }
    }
}

impl OutputArray<{ u8::BITS as usize }> {
    #[inline]
    pub fn set_from_u8(&mut self, mut bits: u8) {
        for output in &mut self.0 {
            let state = (bits & 1) == 1;
            output.set_state(state.into()).unwrap();
            bits >>= 1;
        }
    }
}

pub(crate) struct Pins {
    pub(crate) digits1: OutputArray<CELL_COUNT1>,
    pub(crate) segments1: OutputArray<SEGMENT_COUNT1>,
    pub(crate) button: gpio::Input<'static>,
    _led0: gpio::Output<'static>,
}

impl Pins {
    pub(crate) fn new_and_core1() -> (Self, CORE1) {
        let peripherals: embassy_rp::Peripherals =
            embassy_rp::init(embassy_rp::config::Config::default());
        let core1 = peripherals.CORE1;

        let digits1 = OutputArray::new([
            gpio::Output::new(peripherals.PIN_1, Level::High),
            gpio::Output::new(peripherals.PIN_2, Level::High),
            gpio::Output::new(peripherals.PIN_3, Level::High),
            gpio::Output::new(peripherals.PIN_4, Level::High),
        ]);

        let segments1 = OutputArray::new([
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
                digits1,
                segments1,
                button,
                _led0: led0,
            },
            core1,
        )
    }
}
