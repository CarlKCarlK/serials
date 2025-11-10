use crate::Result;
use crate::display_led4::{DisplayLed4, DisplayLed4Notifier};
use crate::led4::OutputArray;
use crate::led4::{CELL_COUNT, SEGMENT_COUNT};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Duration;

// ============================================================================
// Constants
// ============================================================================

/// Delay for the "off" state during blinking.
const BLINK_OFF_DELAY: Duration = Duration::from_millis(50);

/// Delay for the "on" state during blinking.
const BLINK_ON_DELAY: Duration = Duration::from_millis(150);

// ============================================================================
// BlinkerLed4 Virtual Device
// ============================================================================

/// A device abstraction for a 4-digit 7-segment LED display that supports blinking.
pub struct BlinkerLed4<'a>(&'a BlinkerLed4OuterNotifier);

/// Notifier type for the `BlinkerLed4` device abstraction.
pub type BlinkerLed4Notifier = (BlinkerLed4OuterNotifier, DisplayLed4Notifier);

/// Signal type for sending blink state and text to the `BlinkerLed4` device.
pub type BlinkerLed4OuterNotifier = Signal<CriticalSectionRawMutex, (BlinkState, TextLed4)>;

/// Type alias for 4-character text displayed on a 4-digit LED.
pub type TextLed4 = [char; CELL_COUNT];

/// Blinking behavior for 4-digit LED displays.
///
/// Used with [`Led4::write_text()`](crate::led4::Led4::write_text) to control
/// whether the display blinks. See the [`Led4`](crate::led4::Led4) documentation
/// for usage examples.
#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

impl BlinkerLed4<'_> {
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static BlinkerLed4Notifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (outer_notifier, display_notifier) = notifier;
        let display = DisplayLed4::new(cell_pins, segment_pins, display_notifier, spawner)?;
        let token = device_loop(outer_notifier, display)?;
        spawner.spawn(token);
        Ok(Self(outer_notifier))
    }

    #[must_use]
    pub const fn notifier() -> BlinkerLed4Notifier {
        (Signal::new(), DisplayLed4::notifier())
    }

    pub fn write_text(&self, blink_state: BlinkState, text: TextLed4) {
        #[cfg(feature = "display-trace")]
        info!("blink_state: {:?}, text: {:?}", blink_state, text);
        self.0.signal((blink_state, text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    outer_notifier: &'static BlinkerLed4OuterNotifier,
    display: DisplayLed4<'static>,
) -> ! {
    let mut blink_state = BlinkState::default();
    let mut text = [' '; CELL_COUNT];
    #[expect(clippy::shadow_unrelated, reason = "False positive; not shadowing")]
    loop {
        (blink_state, text) = blink_state.execute(outer_notifier, &display, text).await;
    }
}

impl BlinkState {
    pub async fn execute(
        self,
        outer_notifier: &'static BlinkerLed4OuterNotifier,
        display: &DisplayLed4<'_>,
        text: TextLed4,
    ) -> (Self, TextLed4) {
        use embassy_futures::select::{Either, select};
        use embassy_time::Timer;

        match self {
            Self::Solid => {
                display.write_text(text);
                outer_notifier.wait().await
            }
            Self::BlinkingAndOn => {
                display.write_text(text);
                if let Either::First((new_state, new_text)) =
                    select(outer_notifier.wait(), Timer::after(BLINK_ON_DELAY)).await
                {
                    (new_state, new_text)
                } else {
                    (Self::BlinkingButOff, text)
                }
            }
            Self::BlinkingButOff => {
                display.write_text([' '; CELL_COUNT]);
                if let Either::First((new_state, new_text)) =
                    select(outer_notifier.wait(), Timer::after(BLINK_OFF_DELAY)).await
                {
                    (new_state, new_text)
                } else {
                    (Self::BlinkingAndOn, text)
                }
            }
        }
    }
}
