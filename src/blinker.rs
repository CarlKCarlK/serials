use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

use crate::{
    display::{Display, DisplayNotifier, CELL_COUNT0, SEGMENT_COUNT0},
    pins::OutputArray,
};

const BLINK_OFF_DELAY: Duration = Duration::from_millis(50); // const cmk
const BLINK_ON_DELAY: Duration = Duration::from_millis(150); // const cmk

pub struct Blinker(&'static NotifierInner);
pub type BlinkerNotifier = (NotifierInner, DisplayNotifier<CELL_COUNT0>);
type NotifierInner = Signal<CriticalSectionRawMutex, (BlinkMode, [char; CELL_COUNT0])>;

impl Blinker {
    pub fn new(
        digit_pins: OutputArray<CELL_COUNT0>,
        segment_pins: OutputArray<SEGMENT_COUNT0>,
        notifier: &'static BlinkerNotifier,
        spawner: Spawner,
    ) -> Self {
        let (notifier_inner, display_notifier) = notifier;
        let blinker = Self(notifier_inner);
        let display = Display::new(digit_pins, segment_pins, display_notifier, spawner);
        unwrap!(spawner.spawn(task(display, notifier_inner)));
        blinker
    }

    pub const fn notifier() -> BlinkerNotifier {
        (Signal::new(), Display::notifier())
    }
}

#[embassy_executor::task]
async fn task(display: Display<CELL_COUNT0>, notifier: &'static NotifierInner) -> ! {
    let mut blink_mode = BlinkMode::Solid;
    let mut chars = [' '; CELL_COUNT0];
    loop {
        (blink_mode, chars) = match blink_mode {
            BlinkMode::Solid => {
                display.write_chars(chars);
                notifier.wait().await
            }
            BlinkMode::BlinkingAndOn => {
                display.write_chars(chars);
                if let Either::First((new_blink_mode, new_chars)) =
                    select(notifier.wait(), Timer::after(BLINK_ON_DELAY)).await
                {
                    (new_blink_mode, new_chars)
                } else {
                    (BlinkMode::BlinkingButOff, chars)
                }
            }
            BlinkMode::BlinkingButOff => {
                display.write_chars([' '; CELL_COUNT0]);
                if let Either::First((new_blink_mode, new_chars)) =
                    select(notifier.wait(), Timer::after(BLINK_OFF_DELAY)).await
                {
                    (new_blink_mode, new_chars)
                } else {
                    (BlinkMode::BlinkingAndOn, chars)
                }
            }
        };
    }
}

impl Blinker {
    pub fn write_chars(&self, chars: [char; CELL_COUNT0], blink_mode: BlinkMode) {
        info!("write_chars: {:?}, blink_mode: {:?}", chars, blink_mode);
        self.0.signal((blink_mode, chars));
    }
}

#[derive(Debug, Clone, Copy, defmt::Format)]
pub enum BlinkMode {
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}
