//! Clock4Led virtual device - manages a 4-digit LED clock display
//!
//! This module provides a clock abstraction that displays time on a 4-segment LED display
//! with support for different display modes and UTC offset adjustment.

use core::ops::AddAssign;
use defmt::{info, unwrap};
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Instant, Timer};

use crate::UnixSeconds;
use crate::led_4seg::{BlinkState, Led4Seg, Text};

// ============================================================================
// Constants
// ============================================================================

/// Duration representing one second.
pub const ONE_SECOND: Duration = Duration::from_secs(1);

/// Duration representing one minute (60 seconds).
pub const ONE_MINUTE: Duration = Duration::from_secs(60);

/// Duration representing one hour (60 minutes).
pub const ONE_HOUR: Duration = Duration::from_secs(60 * 60);

/// Duration representing one day (24 hours).
pub const ONE_DAY: Duration = Duration::from_secs(60 * 60 * 24);

/// Duration representing the number of ticks in one day.
pub const TICKS_IN_ONE_DAY: u64 = ONE_DAY.as_ticks();

// ============================================================================
// ClockState - Display modes
// ============================================================================

/// Represents the different states the clock can operate in.
#[derive(Debug, defmt::Format, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClockState {
    /// Display hours and minutes (HH:MM format).
    #[default]
    HoursMinutes,
    /// Display minutes and seconds (MM:SS format).
    MinutesSeconds,
    /// Edit UTC offset mode (blinking display).
    EditUtcOffset,
}

impl ClockState {
    /// Run the clock in the current state and return the next state.
    pub async fn execute(
        self,
        clock: &Clock4Led<'_>,
        button: &mut crate::Button<'_>,
        time_sync: &crate::TimeSync,
    ) -> Self {
        match self {
            Self::HoursMinutes => self.execute_hours_minutes(clock, button, time_sync).await,
            Self::MinutesSeconds => self.execute_minutes_seconds(clock, button, time_sync).await,
            Self::EditUtcOffset => self.execute_edit_utc_offset(clock, button).await,
        }
    }

    async fn execute_hours_minutes(
        self,
        clock: &Clock4Led<'_>,
        button: &mut crate::Button<'_>,
        time_sync: &crate::TimeSync,
    ) -> Self {
        use crate::PressDuration;
        use embassy_futures::select::{Either, select};

        match select(button.press_duration(), time_sync.wait()).await {
            Either::First(PressDuration::Short) => Self::MinutesSeconds,
            Either::First(PressDuration::Long) => Self::EditUtcOffset,
            Either::Second(event) => {
                Self::handle_time_sync_event(clock, event).await;
                self
            }
        }
    }

    async fn execute_minutes_seconds(
        self,
        clock: &Clock4Led<'_>,
        button: &mut crate::Button<'_>,
        time_sync: &crate::TimeSync,
    ) -> Self {
        use crate::PressDuration;
        use embassy_futures::select::{Either, select};

        match select(button.press_duration(), time_sync.wait()).await {
            Either::First(PressDuration::Short) => Self::HoursMinutes,
            Either::First(PressDuration::Long) => Self::EditUtcOffset,
            Either::Second(event) => {
                Self::handle_time_sync_event(clock, event).await;
                self
            }
        }
    }

    async fn execute_edit_utc_offset(
        self,
        clock: &Clock4Led<'_>,
        button: &mut crate::Button<'_>,
    ) -> Self {
        use crate::PressDuration;

        match button.press_duration().await {
            PressDuration::Short => {
                clock.adjust_utc_offset_hours(1).await;
                self
            }
            PressDuration::Long => Self::HoursMinutes,
        }
    }

    async fn handle_time_sync_event(clock: &Clock4Led<'_>, event: crate::TimeSyncEvent) {
        use defmt::info;

        match event {
            crate::TimeSyncEvent::Success { unix_seconds } => {
                info!(
                    "Time sync success: setting clock to {}",
                    unix_seconds.as_i64()
                );
                clock.set_time_from_unix(unix_seconds).await;
            }
            crate::TimeSyncEvent::Failed(msg) => {
                info!("Time sync failed: {}", msg);
            }
        }
    }

    /// Given the current `ClockState` and `ClockTime`, generates display information.
    pub(crate) fn render(self, clock_time: &ClockTime) -> (BlinkState, Text, Duration) {
        match self {
            Self::HoursMinutes => Self::render_hours_minutes(clock_time),
            Self::MinutesSeconds => Self::render_minutes_seconds(clock_time),
            Self::EditUtcOffset => Self::render_edit_utc_offset(clock_time),
        }
    }

    fn render_hours_minutes(clock_time: &ClockTime) -> (BlinkState, Text, Duration) {
        let (hours, minutes, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            BlinkState::Solid,
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
            sleep_duration,
        )
    }

    fn render_minutes_seconds(clock_time: &ClockTime) -> (BlinkState, Text, Duration) {
        let (_, minutes, seconds, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_SECOND);
        (
            BlinkState::Solid,
            [
                tens_digit(minutes),
                ones_digit(minutes),
                tens_digit(seconds),
                ones_digit(seconds),
            ],
            sleep_duration,
        )
    }

    fn render_edit_utc_offset(clock_time: &ClockTime) -> (BlinkState, Text, Duration) {
        let (hours, minutes, _, _) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            BlinkState::BlinkingAndOn,
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
            Duration::from_millis(500),
        )
    }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "value < 60, division is safe"
)]
const fn tens_digit(value: u8) -> char {
    debug_assert!(value < 60);
    ((value / 10) + b'0') as char
}

#[inline]
const fn tens_hours(value: u8) -> char {
    debug_assert!(1 <= value && value <= 12);
    if value >= 10 { '1' } else { ' ' }
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "value < 60, division is safe"
)]
#[inline]
const fn ones_digit(value: u8) -> char {
    debug_assert!(value < 60);
    ((value % 10) + b'0') as char
}

// ============================================================================
// ClockTime - Time tracking
// ============================================================================

/// The system time along with an offset to represent time to display.
pub struct ClockTime {
    offset: Duration,
    /// UTC offset in minutes
    utc_offset_minutes: i32,
}

impl Default for ClockTime {
    fn default() -> Self {
        info!("ClockTime init, now={:?}", Instant::now());
        let utc_offset_minutes = option_env!("UTC_OFFSET_MINUTES")
            .and_then(|val| val.parse::<i32>().ok())
            .unwrap_or(0);
        Self {
            offset: Duration::from_millis(12 * 3600 * 1000), // Start at 12:00:00
            utc_offset_minutes,
        }
    }
}

impl ClockTime {
    /// Sets the time from a Unix timestamp with UTC offset applied.
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "Modulo operations prevent overflow"
    )]
    pub fn set_from_unix(&mut self, unix_seconds: UnixSeconds) {
        let local_seconds = unix_seconds.as_i64() + i64::from(self.utc_offset_minutes) * 60;
        let seconds_since_midnight = (local_seconds % 86400) as u64;
        let millis_since_midnight = seconds_since_midnight * 1000;

        let current_instant_ticks = Instant::now().as_ticks() % TICKS_IN_ONE_DAY;
        let target_ticks =
            Duration::from_millis(millis_since_midnight).as_ticks() % TICKS_IN_ONE_DAY;

        let offset_ticks = if target_ticks >= current_instant_ticks {
            target_ticks - current_instant_ticks
        } else {
            TICKS_IN_ONE_DAY + target_ticks - current_instant_ticks
        };

        self.offset = Duration::from_ticks(offset_ticks % TICKS_IN_ONE_DAY);
        info!(
            "Set time from Unix: {} -> offset: {:?}",
            unix_seconds.as_i64(),
            self.offset.as_millis()
        );
    }

    /// Returns the current time with the offset applied.
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::integer_division_remainder_used,
        reason = "Modulo prevents overflow"
    )]
    #[inline]
    #[must_use]
    pub fn now(&self) -> Duration {
        let ticks = Instant::now().as_ticks() % TICKS_IN_ONE_DAY
            + self.offset.as_ticks() % TICKS_IN_ONE_DAY;
        Duration::from_ticks(ticks % TICKS_IN_ONE_DAY)
    }

    /// Returns hours, minutes, seconds, and wait duration until next unit.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "Modulo operations prevent overflow"
    )]
    #[must_use]
    #[inline]
    pub fn h_m_s_sleep_duration(&self, unit: Duration) -> (u8, u8, u8, Duration) {
        let now = self.now();
        let sleep_duration = Self::till_next(now, unit);
        let elapsed_seconds = now.as_secs();
        let hours = ((elapsed_seconds / 3600) + 11) % 12 + 1; // 1-12 instead of 0-11
        let minutes = (elapsed_seconds % 3600) / 60;
        let seconds = elapsed_seconds % 60;
        (hours as u8, minutes as u8, seconds as u8, sleep_duration)
    }

    #[inline]
    #[must_use]
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "Modulo prevents overflow"
    )]
    pub const fn till_next(time: Duration, unit: Duration) -> Duration {
        let unit_ticks = unit.as_ticks();
        Duration::from_ticks(unit_ticks - time.as_ticks() % unit_ticks)
    }

    /// Returns the current UTC offset in hours (rounded).
    #[expect(
        clippy::integer_division_remainder_used,
        reason = "Division for converting minutes to hours"
    )]
    #[must_use]
    pub fn utc_offset_hours(&self) -> i32 {
        if self.utc_offset_minutes >= 0 {
            (self.utc_offset_minutes + 30) / 60
        } else {
            (self.utc_offset_minutes - 30) / 60
        }
    }

    /// Adjusts the UTC offset by the given number of hours.
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::integer_division_remainder_used,
        reason = "Wrapping arithmetic is intentional"
    )]
    pub fn adjust_utc_offset_hours(&mut self, hours: i32) {
        let current_offset_hours = self.utc_offset_hours();
        let new_offset_hours = current_offset_hours + hours;

        // Wrap around: -12 to +14 (27 values)
        let wrapped = ((new_offset_hours + 12) % 27 + 27) % 27 - 12;
        let delta_hours = wrapped - current_offset_hours;

        if delta_hours >= 0 {
            self.offset += Duration::from_secs((delta_hours * 3600) as u64);
        } else {
            self.offset -= Duration::from_secs(((-delta_hours) * 3600) as u64);
        }

        self.utc_offset_minutes = wrapped * 60;
        info!(
            "Adjusted UTC offset from {} to {} hours (delta: {})",
            current_offset_hours, wrapped, delta_hours
        );
    }
}

impl AddAssign<Duration> for ClockTime {
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "Modulo prevents overflow"
    )]
    fn add_assign(&mut self, duration: Duration) {
        let ticks =
            self.offset.as_ticks() % TICKS_IN_ONE_DAY + duration.as_ticks() % TICKS_IN_ONE_DAY;
        self.offset = Duration::from_ticks(ticks % TICKS_IN_ONE_DAY);
        info!(
            "Now: {:?}, Offset: {:?}",
            Instant::now().as_millis(),
            self.offset.as_millis()
        );
    }
}

// ============================================================================
// ClockCommand - Commands to control the clock
// ============================================================================

pub enum ClockCommand {
    SetState(ClockState),
    SetTimeFromUnix(UnixSeconds),
    AdjustClockTime(Duration),
    ResetSeconds,
    AdjustUtcOffsetHours(i32),
}

impl ClockCommand {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The += operator wraps around to always produce a result less than one day"
    )]
    pub fn apply(self, clock_time: &mut ClockTime, clock_state: &mut ClockState) {
        match self {
            Self::SetTimeFromUnix(unix_seconds) => {
                clock_time.set_from_unix(unix_seconds);
            }
            Self::AdjustClockTime(delta) => {
                *clock_time += delta;
            }
            Self::SetState(new_state) => {
                *clock_state = new_state;
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

// ============================================================================
// Clock4Led Virtual Device
// ============================================================================

/// A clock abstraction that displays time on a 4-digit LED display.
pub struct Clock4Led<'a>(&'a Clock4LedNotifier);

/// Notifier for sending commands to the Clock4Led device.
pub type Clock4LedNotifier = Channel<CriticalSectionRawMutex, ClockCommand, 4>;

impl Clock4Led<'_> {
    /// Creates a new `Clock4LedNotifier`.
    #[must_use]
    pub const fn notifier() -> Clock4LedNotifier {
        Channel::new()
    }

    /// Creates a new `Clock4Led` device.
    ///
    /// # Arguments
    ///
    /// * `led_display` - The LED4Seg display device to use.
    /// * `notifier` - The static notifier for sending commands.
    /// * `spawner` - The Embassy task spawner.
    ///
    /// # Errors
    ///
    /// Returns a `SpawnError` if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        led_display: &'static Led4Seg<'static>,
        notifier: &'static Clock4LedNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = unwrap!(clock_4led_device_loop(led_display, notifier));
        spawner.spawn(token);
        Ok(Self(notifier))
    }

    /// Sets the clock state (display mode).
    pub async fn set_state(&self, clock_state: ClockState) {
        self.0.send(ClockCommand::SetState(clock_state)).await;
    }

    /// Sets the time from a Unix timestamp.
    pub async fn set_time_from_unix(&self, unix_seconds: UnixSeconds) {
        self.0
            .send(ClockCommand::SetTimeFromUnix(unix_seconds))
            .await;
    }

    /// Adjusts the UTC offset by the given number of hours.
    pub async fn adjust_utc_offset_hours(&self, hours: i32) {
        self.0.send(ClockCommand::AdjustUtcOffsetHours(hours)).await;
    }
}

#[embassy_executor::task]
async fn clock_4led_device_loop(
    led_display: &'static Led4Seg<'static>,
    notifier: &'static Clock4LedNotifier,
) -> ! {
    let mut clock_time = ClockTime::default();
    let mut clock_state = ClockState::default();

    loop {
        // Check for any pending commands before rendering
        while let Ok(command) = notifier.try_receive() {
            command.apply(&mut clock_time, &mut clock_state);
        }

        // Compute display and time until display change
        let (blink_mode, text, sleep_duration) = clock_state.render(&clock_time);
        led_display.write_text(blink_mode, text);

        info!("Sleep for {:?}", sleep_duration);

        // Sleep or wait for command
        if let Either::First(command) =
            select(notifier.receive(), Timer::after(sleep_duration)).await
        {
            command.apply(&mut clock_time, &mut clock_state);
        }
    }
}
