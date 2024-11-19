use crate::virtual_led::DIGIT_COUNT1;
use embassy_rp::{
    gpio::{self, Level},
    peripherals::CORE1,
};

pub(crate) struct Pins {
    pub(crate) digits1: [gpio::Output<'static>; DIGIT_COUNT1],
    pub(crate) segments1: [gpio::Output<'static>; 8],
    pub(crate) button: gpio::Input<'static>,
    _led0: gpio::Output<'static>,
}

impl Pins {
    pub(crate) fn new_and_core1() -> (Self, CORE1) {
        let peripherals: embassy_rp::Peripherals =
            embassy_rp::init(embassy_rp::config::Config::default());
        let core1 = peripherals.CORE1;

        let digits1 = [
            gpio::Output::new(peripherals.PIN_1, Level::High),
            gpio::Output::new(peripherals.PIN_2, Level::High),
            gpio::Output::new(peripherals.PIN_3, Level::High),
            gpio::Output::new(peripherals.PIN_4, Level::High),
        ];

        let segments1 = [
            gpio::Output::new(peripherals.PIN_5, Level::Low),
            gpio::Output::new(peripherals.PIN_6, Level::Low),
            gpio::Output::new(peripherals.PIN_7, Level::Low),
            gpio::Output::new(peripherals.PIN_8, Level::Low),
            gpio::Output::new(peripherals.PIN_9, Level::Low),
            gpio::Output::new(peripherals.PIN_10, Level::Low),
            gpio::Output::new(peripherals.PIN_11, Level::Low),
            gpio::Output::new(peripherals.PIN_12, Level::Low),
        ];

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
