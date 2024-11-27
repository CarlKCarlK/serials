use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;

use crate::{
    display::{Display, DisplayNotifier},
    output_array::OutputArray,
    shared_constants::{BLINK_OFF_DELAY, BLINK_ON_DELAY, CELL_COUNT, SEGMENT_COUNT},
};

pub struct Blinker<'a>(&'a NotifierInner);
pub type BlinkerNotifier = (NotifierInner, DisplayNotifier);
type NotifierInner = Signal<CriticalSectionRawMutex, (BlinkMode, [char; CELL_COUNT])>;

impl Blinker<'_> {
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        digit_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static BlinkerNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (notifier_inner, display_notifier) = notifier;
        let blinker = Self(notifier_inner);
        let display = Display::new(digit_pins, segment_pins, display_notifier, spawner)?;
        spawner.spawn(device_loop(display, notifier_inner))?;
        Ok(blinker)
    }

    pub const fn notifier() -> BlinkerNotifier {
        (Signal::new(), Display::notifier())
    }
}

#[embassy_executor::task]
async fn device_loop(display: Display<'static>, notifier: &'static NotifierInner) -> ! {
    let mut blink_mode = BlinkMode::Solid;
    let mut chars = [' '; CELL_COUNT];
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
                display.write_chars([' '; CELL_COUNT]);
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

impl Blinker<'_> {
    pub fn write_chars(&self, chars: [char; CELL_COUNT], blink_mode: BlinkMode) {
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
