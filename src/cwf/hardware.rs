use embassy_rp::{
    Peripherals,
    gpio::{self, Level},
};

use crate::cwf::output_array::OutputArray;
use crate::cwf::shared_constants::{CELL_COUNT, SEGMENT_COUNT};

/// Represents the hardware components of the 4-digit clock.
pub struct Hardware {
    /// The four cell pins that control the digits of the display.
    pub cells: OutputArray<'static, CELL_COUNT>,
    /// The eight segment pins that control the segments of the display.
    pub segments: OutputArray<'static, SEGMENT_COUNT>,
    /// The button that controls the clock.
    pub button: gpio::Input<'static>,
    /// An on-board LED for debugging (currently unused).
    pub led: gpio::Output<'static>,
}

impl Default for Hardware {
    fn default() -> Self {
        let peripherals: Peripherals = embassy_rp::init(Default::default());

        let led = gpio::Output::new(peripherals.PIN_0, Level::Low);

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

        Self {
            cells,
            segments,
            button,
            led,
        }
    }
}
