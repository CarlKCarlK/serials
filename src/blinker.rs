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

/// A struct representing a display with the ability to blink.
pub struct Blinker<'a>(&'a NotifierInner);

/// A type alias for the notifier that sends messages to the `Blinker`
/// and the `Display` it controls.
pub type BlinkerNotifier = (NotifierInner, DisplayNotifier);

/// A type alias for the inner notifier that sends messages to the `Blinker`.
type NotifierInner = Signal<CriticalSectionRawMutex, (BlinkMode, [char; CELL_COUNT])>;

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
        spawner.spawn(device_loop(display, notifier_inner))?;
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
    pub fn write_chars(&self, chars: [char; CELL_COUNT], blink_mode: BlinkMode) {
        info!("write_chars: {:?}, blink_mode: {:?}", chars, blink_mode);
        self.0.signal((blink_mode, chars));
    }
}

#[embassy_executor::task]
async fn device_loop(display: Display<'static>, notifier: &'static NotifierInner) -> ! {
    let mut blink_mode = BlinkMode::default();
    let mut chars = [' '; CELL_COUNT];
    #[expect(clippy::shadow_unrelated, reason = "This is a false positive.")]
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

#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkMode {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}
