use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

use crate::{
    blinker::{Blinker, BlinkerNotifier},
    clock_time::ClockTime,
    output_array::OutputArray,
    shared_constants::{CELL_COUNT, ONE_MINUTE, SEGMENT_COUNT},
    ClockState,
};

/// A struct representing a virtual clock.
pub struct Clock<'a>(&'a NotifierInner);
/// Type alias for notifier that sends messages to the `Clock` and the `Blinker` it controls.
pub type ClockNotifier = (NotifierInner, BlinkerNotifier);
/// A type alias for the inner notifier that sends messages to the `Clock`.
///
/// The number is the maximum number of messages that can be stored in the channel without blocking.
pub type NotifierInner = Channel<CriticalSectionRawMutex, ClockNotice, 4>;

impl Clock<'_> {
    /// Creates a new `Clock` instance. cmk
    ///
    /// # Errors
    ///
    /// Returns a `SpawnError` if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static ClockNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (notifier_inner, blinker_notifier) = notifier;
        let clock = Self(notifier_inner);
        let blinkable_display = Blinker::new(cell_pins, segment_pins, blinker_notifier, spawner)?;
        spawner.spawn(device_loop(blinkable_display, notifier_inner))?;
        Ok(clock)
    }

    #[must_use]
    /// Creates a new `ClockNotifier` instance.
    pub const fn notifier() -> ClockNotifier {
        (Channel::new(), Blinker::notifier())
    }

    pub(crate) async fn set_mode(&self, clock_state: ClockState) {
        self.0.send(ClockNotice::SetMode { clock_state }).await;
    }

    pub(crate) async fn adjust_offset(&self, delta: Duration) {
        self.0.send(ClockNotice::AdjustOffset(delta)).await;
    }

    pub(crate) async fn reset_seconds(&self) {
        self.0.send(ClockNotice::ResetSeconds).await;
    }
}

pub enum ClockNotice {
    SetMode { clock_state: ClockState },
    AdjustOffset(Duration),
    ResetSeconds,
}

impl ClockNotice {
    /// Handles the action associated with the given `ClockNotice`.
    pub(crate) fn apply(self, clock_time: &mut ClockTime, clock_state: &mut ClockState) {
        match self {
            Self::AdjustOffset(delta) => {
                *clock_time += delta;
            }
            Self::SetMode {
                clock_state: new_clock_mode,
            } => {
                *clock_state = new_clock_mode;
            }
            Self::ResetSeconds => {
                let sleep_duration = ClockTime::till_next(clock_time.now(), ONE_MINUTE);
                *clock_time += sleep_duration;
            }
        }
    }
}

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn device_loop(
    blinkable_display: Blinker<'static>,
    clock_notifier: &'static NotifierInner,
) -> ! {
    let mut clock_time = ClockTime::default();
    let mut clock_state = ClockState::default();

    loop {
        // Compute the display and time until the display change.
        let (chars, blink_mode, sleep_duration) = clock_state.render(&clock_time);
        blinkable_display.write_chars(chars, blink_mode);

        // Wait for a notification or for the sleep duration to elapse
        info!("Sleep for {:?}", sleep_duration);
        if let Either::First(notification) =
            select(clock_notifier.receive(), Timer::after(sleep_duration)).await
        {
            notification.apply(&mut clock_time, &mut clock_state);
        }
    }
}
