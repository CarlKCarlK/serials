use crate::Result;
use crate::BlinkState;
use crate::clock_4led_display::{Display, DisplayNotifier};
use crate::OutputArray;
use crate::clock_4led_constants::{CELL_COUNT, SEGMENT_COUNT};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

pub struct Blinker<'a>(&'a BlinkerOuterNotifier);

pub type BlinkerNotifier = (BlinkerOuterNotifier, DisplayNotifier);

pub type BlinkerOuterNotifier = Signal<CriticalSectionRawMutex, (BlinkState, Text)>;

pub type Text = [char; CELL_COUNT];

impl Blinker<'_> {
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static BlinkerNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (outer_notifier, display_notifier) = notifier;
        let display = Display::new(cell_pins, segment_pins, display_notifier, spawner)?;
        let token = device_loop(outer_notifier, display)?;
        spawner.spawn(token);
        Ok(Self(outer_notifier))
    }

    #[must_use]
    pub const fn notifier() -> BlinkerNotifier {
        (Signal::new(), Display::notifier())
    }

    pub fn write_text(&self, blink_state: BlinkState, text: Text) {
        #[cfg(feature = "display-trace")]
        info!("blink_state: {:?}, text: {:?}", blink_state, text);
        self.0.signal((blink_state, text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    outer_notifier: &'static BlinkerOuterNotifier,
    display: Display<'static>,
) -> ! {
    let mut blink_state = BlinkState::default();
    let mut text = [' '; CELL_COUNT];
    #[expect(clippy::shadow_unrelated, reason = "False positive; not shadowing")]
    loop {
        (blink_state, text) = blink_state.execute(outer_notifier, &display, text).await;
    }
}
