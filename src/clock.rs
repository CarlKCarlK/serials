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
    /// Create a new `Clock` instance, which entails starting an Embassy task.
    ///
    /// # Arguments
    ///
    /// * `cell_pins` - The pins that control the cells (digits) of the display.
    /// * `segment_pins` - The pins that control the segments of the display.
    /// * `notifier` - The static notifier that sends messages to the `Clock` and the `Blinker` it controls.
    ///          This notifier is created with the `Clock::notifier()` method.
    /// * `spawner` - The spawner that will spawn the task that controls the clock.
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
        let blinkable_display = Blinker::new(cell_pins, segment_pins, blinker_notifier, spawner)?;
        spawner.spawn(device_loop(blinkable_display, notifier_inner))?;
        Ok(Self(notifier_inner))
    }

    /// Creates a new `ClockNotifier` instance.
    ///
    /// This notifier is used to send messages to the `Clock` and the `Blinker` it controls.
    ///
    /// The `ClockNotifier` instance should be assigned to a static variable and passed
    /// to the `Clock::new()` method.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[expect(clippy::items_after_statements, reason = "Keeps related code together")]
    /// static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    /// let mut clock = Clock::new(hardware.cells, hardware.segments, &CLOCK_NOTIFIER, spawner)?;
    /// ```
    #[must_use]
    pub const fn notifier() -> ClockNotifier {
        (Channel::new(), Blinker::notifier())
    }

    pub(crate) async fn set_state(&self, clock_state: ClockState) {
        self.0.send(ClockNotice::SetState { clock_state }).await;
    }

    pub(crate) async fn adjust_offset(&self, delta: Duration) {
        self.0.send(ClockNotice::AdjustClockTime(delta)).await;
    }

    pub(crate) async fn reset_seconds(&self) {
        self.0.send(ClockNotice::ResetSeconds).await;
    }
}

pub enum ClockNotice {
    SetState { clock_state: ClockState },
    AdjustClockTime(Duration),
    ResetSeconds,
}

impl ClockNotice {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The += operator wraps around to always produce a result less than one day."
    )]
    /// Handles the action associated with the given `ClockNotice`.
    pub(crate) fn apply(self, clock_time: &mut ClockTime, clock_state: &mut ClockState) {
        match self {
            Self::AdjustClockTime(delta) => {
                *clock_time += delta;
            }
            Self::SetState {
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

// cmk make sure dua-blinka does Ok(Self(notifier_inner))
