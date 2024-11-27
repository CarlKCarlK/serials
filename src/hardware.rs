use embassy_rp::{
    gpio::{self, Level},
    peripherals::CORE1,
};

use crate::{output_array::OutputArray, CELL_COUNT, SEGMENT_COUNT};

pub struct Hardware {
    pub cells: OutputArray<'static, CELL_COUNT>,
    pub segments: OutputArray<'static, SEGMENT_COUNT>,
    pub button: gpio::Input<'static>,
    pub led: gpio::Output<'static>,
    pub core1: CORE1,
}

impl Default for Hardware {
    fn default() -> Self {
        let peripherals: embassy_rp::Peripherals =
            embassy_rp::init(embassy_rp::config::Config::default());

        let cells = OutputArray::new([
            gpio::Output::new(peripherals.PIN_1, Level::High),
            gpio::Output::new(peripherals.PIN_2, Level::High),
            gpio::Output::new(peripherals.PIN_3, Level::High),
            gpio::Output::new(peripherals.PIN_4, Level::High),
        ]);

        let segments = OutputArray::new([
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

        let led = gpio::Output::new(peripherals.PIN_0, Level::Low);

        let core1 = peripherals.CORE1;

        Self {
            cells,
            segments,
            button,
            led,
            core1,
        }
    }
}
