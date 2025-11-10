use crate::Result;
use crate::display_4led::{Display4Led, Display4LedNotifier};
use crate::led_4seg::OutputArray;
use crate::constants::{CELL_COUNT_4LED, SEGMENT_COUNT_4LED};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

/// A device abstraction for a 4-digit 7-segment LED display that supports blinking.
pub struct Blinker4Led<'a>(&'a Blinker4LedOuterNotifier);

/// Notifier type for the `Blinker4Led` device abstraction.
pub type Blinker4LedNotifier = (Blinker4LedOuterNotifier, Display4LedNotifier);

/// Signal type for sending blink state and text to the `Blinker4Led` device.
pub type Blinker4LedOuterNotifier = Signal<CriticalSectionRawMutex, (BlinkState4Led, Text4Led)>;

/// Type alias for 4-character text displayed on a 4-digit LED.
pub type Text4Led = [char; CELL_COUNT_4LED];

/// Blinking behavior for 4-digit LED displays.
#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState4Led {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

impl Blinker4Led<'_> {
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT_4LED>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT_4LED>,
        notifier: &'static Blinker4LedNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (outer_notifier, display_notifier) = notifier;
        let display = Display4Led::new(cell_pins, segment_pins, display_notifier, spawner)?;
        let token = device_loop(outer_notifier, display)?;
        spawner.spawn(token);
        Ok(Self(outer_notifier))
    }

    #[must_use]
    pub const fn notifier() -> Blinker4LedNotifier {
        (Signal::new(), Display4Led::notifier())
    }

    pub fn write_text(&self, blink_state: BlinkState4Led, text: Text4Led) {
        #[cfg(feature = "display-trace")]
        info!("blink_state: {:?}, text: {:?}", blink_state, text);
        self.0.signal((blink_state, text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    outer_notifier: &'static Blinker4LedOuterNotifier,
    display: Display4Led<'static>,
) -> ! {
    let mut blink_state = BlinkState4Led::default();
    let mut text = [' '; CELL_COUNT_4LED];
    #[expect(clippy::shadow_unrelated, reason = "False positive; not shadowing")]
    loop {
        (blink_state, text) = blink_state.execute(outer_notifier, &display, text).await;
    }
}

impl BlinkState4Led {
    pub async fn execute(
        self,
        outer_notifier: &'static Blinker4LedOuterNotifier,
        display: &Display4Led<'_>,
        text: Text4Led,
    ) -> (Self, Text4Led) {
        use embassy_futures::select::{Either, select};
        use embassy_time::Timer;
        use crate::constants::{BLINK_OFF_DELAY_4LED, BLINK_ON_DELAY_4LED};

        match self {
            Self::Solid => {
                display.write_text(text);
                outer_notifier.wait().await
            }
            Self::BlinkingAndOn => {
                display.write_text(text);
                if let Either::First((new_state, new_text)) =
                    select(outer_notifier.wait(), Timer::after(BLINK_ON_DELAY_4LED)).await
                {
                    (new_state, new_text)
                } else {
                    (Self::BlinkingButOff, text)
                }
            }
            Self::BlinkingButOff => {
                display.write_text([' '; CELL_COUNT_4LED]);
                if let Either::First((new_state, new_text)) =
                    select(outer_notifier.wait(), Timer::after(BLINK_OFF_DELAY_4LED)).await
                {
                    (new_state, new_text)
                } else {
                    (Self::BlinkingAndOn, text)
                }
            }
        }
    }
}
