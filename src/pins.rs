use embassy_rp::{
    gpio::{self, Level},
    peripherals::CORE1,
};
use static_cell::StaticCell;

use crate::virtual_led::DIGIT_COUNT1;

pub(crate) struct Pins {
    pub(crate) digits1: &'static mut [gpio::Output<'static>; DIGIT_COUNT1],
    pub(crate) segments1: &'static mut [gpio::Output<'static>; 8],
    pub(crate) button: &'static mut gpio::Input<'static>,
    _led0: &'static mut gpio::Output<'static>,
}

static DIGIT_PINS1: StaticCell<[gpio::Output; DIGIT_COUNT1]> = StaticCell::new();
static SEGMENT_PINS1: StaticCell<[gpio::Output; 8]> = StaticCell::new();
static BUTTON_PIN: StaticCell<gpio::Input> = StaticCell::new();
static LED0_PIN: StaticCell<gpio::Output> = StaticCell::new();

impl Pins {
    pub(crate) fn new_and_core1() -> (Self, CORE1) {
        let p: embassy_rp::Peripherals = embassy_rp::init(embassy_rp::config::Config::default());
        let core1 = p.CORE1;

        let digits1 = DIGIT_PINS1.init([
            gpio::Output::new(p.PIN_1, Level::High),
            gpio::Output::new(p.PIN_2, Level::High),
            gpio::Output::new(p.PIN_3, Level::High),
            gpio::Output::new(p.PIN_4, Level::High),
        ]);

        let segments1 = SEGMENT_PINS1.init([
            gpio::Output::new(p.PIN_5, Level::Low),
            gpio::Output::new(p.PIN_6, Level::Low),
            gpio::Output::new(p.PIN_7, Level::Low),
            gpio::Output::new(p.PIN_8, Level::Low),
            gpio::Output::new(p.PIN_9, Level::Low),
            gpio::Output::new(p.PIN_10, Level::Low),
            gpio::Output::new(p.PIN_11, Level::Low),
            gpio::Output::new(p.PIN_12, Level::Low),
        ]);

        let button = BUTTON_PIN.init(gpio::Input::new(p.PIN_13, gpio::Pull::Down));

        let led0 = LED0_PIN.init(gpio::Output::new(p.PIN_0, Level::Low));

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
