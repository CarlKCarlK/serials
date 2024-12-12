use crate::{
    blink_state::BlinkState,
    display::{Display, DisplayNotifier},
    output_array::OutputArray,
    shared_constants::{CELL_COUNT, SEGMENT_COUNT},
};
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

/// A struct representing a display with the ability to blink.
pub struct Blinker<'a>(&'a NotifierInner);

/// A type alias for the notifier that sends messages to the `Blinker`
/// and the `Display` it controls.
pub type BlinkerNotifier = (NotifierInner, DisplayNotifier);

// cmk give a more descriptive name to the inner notifier
/// A type alias for the inner notifier that sends messages to the `Blinker`.
pub type NotifierInner = Signal<CriticalSectionRawMutex, (BlinkState, Text)>;

pub type Text = [char; CELL_COUNT];

impl Blinker<'_> {
    /// Creates a new `Blinker` instance, which entails starting an Embassy task.
    ///
    /// # Arguments
    ///
    /// * `cell_pins` - The pins that control the cells (digits) of the display.
    /// * `segment_pins` - The pins that control the segments of the display.
    /// * `notifier` - The static notifier that sends messages to the `Blinker` and the `Display` it controls.
    ///         This notifier is created with the `Blinker::notifier()` method.
    /// * `spawner` - The spawner that will spawn the task that controls the blinker.
    ///
    /// # Errors
    ///
    /// Returns a `SpawnError` if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static BlinkerNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (notifier_inner, display_notifier) = notifier;
        let display = Display::new(cell_pins, segment_pins, display_notifier, spawner)?;
        spawner.spawn(device_loop(notifier_inner, display))?;
        Ok(Self(notifier_inner))
    }

    /// Creates a new `BlinkerNotifier` instance.
    ///
    /// This notifier is used to send messages to the `Blinker` and the `Display` it controls.
    ///
    /// This should be assigned to a static variable and passed to the `Blinker::new()` method.
    #[must_use]
    pub const fn notifier() -> BlinkerNotifier {
        (Signal::new(), Display::notifier())
    }

    /// Writes possibly-blinking characters to the blinkable display.
    ///
    /// The characters can be be any Unicode character but
    /// an unknown or hard-to-display character will be displayed as a blank.
    pub fn write_text(&self, blink_state: BlinkState, text: Text) {
        info!("blink_state: {:?}, text: {:?}", blink_state, text);
        self.0.signal((blink_state, text));
    }
}

// cmk why no inner_define_loop
// cmk use inline helper functions in the match.

#[embassy_executor::task]
async fn device_loop(notifier_inner: &'static NotifierInner, display: Display<'static>) -> ! {
    let mut blink_state = BlinkState::default();
    let mut text = [' '; CELL_COUNT];
    #[expect(clippy::shadow_unrelated, reason = "false positive. Not shadowing.")]
    loop {
        (blink_state, text) = blink_state
            .run_and_next(notifier_inner, &display, text)
            .await;
    }
}
