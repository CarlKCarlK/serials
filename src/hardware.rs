use embassy_rp::{
    gpio::{self, Level},
    peripherals::CORE1,
    Peri,
};

use crate::{output_array::OutputArray, CELL_COUNT, SEGMENT_COUNT};

/// Represents the hardware components of the clock.
pub struct Hardware {
    // TODO replace the 'static's with <'a> lifetimes
    /// The four cell pins that control the digits of the display.
    pub cells: OutputArray<'static, CELL_COUNT>,
    /// The eight segment pins that control the segments of the display.
    pub segments: OutputArray<'static, SEGMENT_COUNT>,
    /// The button that controls the clock.
    pub button: gpio::Input<'static>,
    /// An LED (not currently used).
    pub led: gpio::Output<'static>,
    /// The second core of the RP2040 (not currently used).
    pub core1: Peri<'static, CORE1>,
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
