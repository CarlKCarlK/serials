//! A device abstraction for 4-digit LED clocks.

pub mod state;
pub mod time;

#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

use crate::blinker_led4::{BlinkerLed4, BlinkerLed4Notifier};
use self::state::ClockLed4State;
use self::time::ClockTime;
use crate::led4::OutputArray;
use crate::constants::{CELL_COUNT_LED4, ONE_MINUTE, SEGMENT_COUNT_LED4};

/// A device abstraction for a 4-digit LED clock.
pub struct ClockLed4<'a>(&'a ClockLed4OuterNotifier);
/// Notifier type for the `ClockLed4` device abstraction.
pub type ClockLed4Notifier = (ClockLed4OuterNotifier, BlinkerLed4Notifier);
/// Channel type for sending commands to the `ClockLed4` device.
pub type ClockLed4OuterNotifier = Channel<CriticalSectionRawMutex, ClockLed4Command, 4>;

impl ClockLed4<'_> {
    /// Create a new `ClockLed4` instance, which entails starting an Embassy task.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT_LED4>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT_LED4>,
        notifier: &'static ClockLed4Notifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (outer_notifier, blinker_notifier) = notifier;
        let blinkable_display = BlinkerLed4::new(cell_pins, segment_pins, blinker_notifier, spawner)?;
        let token = clock_led4_device_loop(outer_notifier, blinkable_display)?;
        spawner.spawn(token);
        Ok(Self(outer_notifier))
    }

    /// Creates a new `ClockLed4Notifier` instance.
    #[must_use]
    pub const fn notifier() -> ClockLed4Notifier {
        (Channel::new(), BlinkerLed4::notifier())
    }

    /// Set the clock state directly.
    pub async fn set_state(&self, clock_state: ClockLed4State) {
        self.0.send(ClockLed4Command::SetState(clock_state)).await;
    }

    /// Set the time from Unix seconds.
    pub async fn set_time_from_unix(&self, unix_seconds: crate::unix_seconds::UnixSeconds) {
        self.0
            .send(ClockLed4Command::SetTimeFromUnix(unix_seconds))
            .await;
    }

    /// Adjust the UTC offset by the given number of hours.
    pub async fn adjust_utc_offset_hours(&self, hours: i32) {
        self.0.send(ClockLed4Command::AdjustUtcOffsetHours(hours)).await;
    }

    /// Display the completion message for flash-clearing workflows.
    pub async fn show_clearing_done(&self) {
        self.0
            .send(ClockLed4Command::SetState(ClockLed4State::ClearingDone))
            .await;
    }

    /// Display the access point setup prompt while waiting for credentials.
    pub async fn show_access_point_setup(&self) {
        self.0
            .send(ClockLed4Command::SetState(ClockLed4State::AccessPointSetup))
            .await;
    }
}

/// Commands sent to the 4-digit LED clock device.
pub enum ClockLed4Command {
    SetState(ClockLed4State),
    SetTimeFromUnix(crate::unix_seconds::UnixSeconds),
    AdjustClockTime(Duration),
    ResetSeconds,
    AdjustUtcOffsetHours(i32),
}

impl ClockLed4Command {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The += operator wraps to always produce a result less than one day."
    )]
    pub(crate) fn apply(self, clock_time: &mut ClockTime, clock_state: &mut ClockLed4State) {
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
async fn clock_led4_device_loop(clock_notifier: &'static ClockLed4OuterNotifier, blinker: BlinkerLed4<'static>) -> ! {
    let mut clock_time = ClockTime::default();
    let mut clock_state = ClockLed4State::default();

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
