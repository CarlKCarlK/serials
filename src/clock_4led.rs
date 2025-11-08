//! Clock4Led - 4-digit LED clock with state machine and button controls

#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

use crate::blinker_4led::{Blinker4Led, Blinker4LedNotifier};
use crate::Clock4LedState;
use crate::clock_4led_time::ClockTime;
use crate::OutputArray;
use crate::constants::{CELL_COUNT_4LED, ONE_MINUTE, SEGMENT_COUNT_4LED};

/// A struct representing a 4-digit LED clock.
pub struct Clock4Led<'a>(&'a Clock4LedOuterNotifier);
/// Type alias for notifier that sends messages to the `Clock4Led` and the `Blinker` it controls.
pub type Clock4LedNotifier = (Clock4LedOuterNotifier, Blinker4LedNotifier);
/// A type alias for the outer notifier that sends messages to the `Clock4Led`.
///
/// The usize parameter is the maximum number of messages that can be stored in the channel without blocking.
pub type Clock4LedOuterNotifier = Channel<CriticalSectionRawMutex, Clock4LedCommand, 4>;

impl Clock4Led<'_> {
    /// Create a new `Clock4Led` instance, which entails starting an Embassy task.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT_4LED>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT_4LED>,
        notifier: &'static Clock4LedNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (outer_notifier, blinker_notifier) = notifier;
        let blinkable_display = Blinker4Led::new(cell_pins, segment_pins, blinker_notifier, spawner)?;
        let token = clock_4led_device_loop(outer_notifier, blinkable_display)?;
        spawner.spawn(token);
        Ok(Self(outer_notifier))
    }

    /// Creates a new `Clock4LedNotifier` instance.
    #[must_use]
    pub const fn notifier() -> Clock4LedNotifier {
        (Channel::new(), Blinker4Led::notifier())
    }

    /// Set the clock state directly.
    pub async fn set_state(&self, clock_state: Clock4LedState) {
        self.0.send(Clock4LedCommand::SetState(clock_state)).await;
    }

    /// Set the time from Unix seconds.
    pub async fn set_time_from_unix(&self, unix_seconds: crate::unix_seconds::UnixSeconds) {
        self.0
            .send(Clock4LedCommand::SetTimeFromUnix(unix_seconds))
            .await;
    }

    /// Adjust the UTC offset by the given number of hours.
    pub async fn adjust_utc_offset_hours(&self, hours: i32) {
        self.0.send(Clock4LedCommand::AdjustUtcOffsetHours(hours)).await;
    }

    /// Display the completion message for flash-clearing workflows.
    pub async fn show_clearing_done(&self) {
        self.0
            .send(Clock4LedCommand::SetState(Clock4LedState::ClearingDone))
            .await;
    }

    /// Display the access point setup prompt while waiting for credentials.
    pub async fn show_access_point_setup(&self) {
        self.0
            .send(Clock4LedCommand::SetState(Clock4LedState::AccessPointSetup))
            .await;
    }
}

pub enum Clock4LedCommand {
    SetState(Clock4LedState),
    SetTimeFromUnix(crate::unix_seconds::UnixSeconds),
    AdjustClockTime(Duration),
    ResetSeconds,
    AdjustUtcOffsetHours(i32),
}

impl Clock4LedCommand {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The += operator wraps to always produce a result less than one day."
    )]
    pub(crate) fn apply(self, clock_time: &mut ClockTime, clock_state: &mut Clock4LedState) {
        match self {
            Self::SetTimeFromUnix(unix_seconds) => {
                clock_time.set_from_unix(unix_seconds);
            }
            Self::AdjustClockTime(delta) => {
                *clock_time += delta;
            }
            Self::SetState(new_clock_mode) => {
                *clock_state = new_clock_mode;
            }
            Self::ResetSeconds => {
                let sleep_duration = ClockTime::till_next(clock_time.now(), ONE_MINUTE);
                *clock_time += sleep_duration;
            }
            Self::AdjustUtcOffsetHours(hours) => {
                clock_time.adjust_utc_offset_hours(hours);
            }
        }
    }
}

#[embassy_executor::task]
async fn clock_4led_device_loop(clock_notifier: &'static Clock4LedOuterNotifier, blinker: Blinker4Led<'static>) -> ! {
    let mut clock_time = ClockTime::default();
    let mut clock_state = Clock4LedState::default();

    loop {
        let (blink_mode, text, sleep_duration) = clock_state.render(&clock_time);
        blinker.write_text(blink_mode, text);

        #[cfg(feature = "display-trace")]
        info!("Sleep for {:?}", sleep_duration);
        if let Either::First(notification) =
            select(clock_notifier.receive(), Timer::after(sleep_duration)).await
        {
            notification.apply(&mut clock_time, &mut clock_state);
        }
    }
}
