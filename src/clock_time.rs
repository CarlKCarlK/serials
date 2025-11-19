//! A device abstraction for lightweight time tracking on embedded displays.
//!
//! This module provides simple time-of-day tracking using [`Instant::now()`](embassy_time::Instant::now) plus an offset.
//! The clock tracks local time by applying a UTC offset, and can be synchronized via
//! Unix timestamps from network time protocols.
//!
//! Unlike most device abstractions in this crate, [`ClockTime`] does not require static
//! resources and can be instantiated directly.
//!
//! # Example
//!
//! See [`ClockTime`] for a complete example.

use defmt::info;
use embassy_time::{Duration, Instant};

use crate::unix_seconds::UnixSeconds;

/// Duration representing one second.
pub const ONE_SECOND: Duration = Duration::from_secs(1);
/// Duration representing one minute (60 seconds).
pub const ONE_MINUTE: Duration = Duration::from_secs(60);
/// Duration representing one day (24 hours).
pub const ONE_DAY: Duration = Duration::from_secs(60 * 60 * 24);

/// Number of ticks in one day.
const TICKS_IN_ONE_DAY: u64 = ONE_DAY.as_ticks();

/// A device abstraction for lightweight time tracking on embedded displays.
///
/// Maintains local time-of-day using [`Instant::now()`](embassy_time::Instant::now) plus an offset. The clock is
/// initialized with a UTC offset (in minutes) to specify the local timezone, then
/// synchronized to the correct time via [`set_from_unix()`](ClockTime::set_from_unix).
///
/// Unlike most device abstractions in this crate, `ClockTime` does not require static resources.
///
/// # Example
///
/// ```no_run
/// # use serials::clock_time::{ClockTime, ONE_MINUTE};
/// # use serials::unix_seconds::UnixSeconds;
/// # use embassy_time::Duration;
/// #
/// // Create a clock with UTC-8 offset (PST)
/// let mut clock = ClockTime::new(-8 * 60);
///
/// // Later, synchronize to actual time from Network Time Protocol (NTP)
/// # let unix_time = UnixSeconds(1700000000);
/// clock.set_from_unix(unix_time);
///
/// // Now we can display the current local time
/// // The sleep_duration tells us how long we can sleep before the display needs updating
/// let (hours, minutes, seconds, sleep_duration) = clock.h_m_s_sleep_duration(ONE_MINUTE);
///
/// // User can change UTC offset (e.g., when traveling to MST)
/// clock.set_offset_minutes(-7 * 60);
///
/// // Verify the new offset is -7 hours * 60 minutes
/// let offset = clock.offset_minutes();
/// # let _ = (offset, hours, minutes, seconds, sleep_duration);
/// ```
pub struct ClockTime {
    offset: Duration,
    offset_minutes: i32,
}

impl ClockTime {
    /// Create a new `ClockTime` with the given UTC offset in minutes.
    ///
    /// This sets the UTC offset that will be applied when converting UTC timestamps
    /// to local time. Call [`set_from_unix()`](ClockTime::set_from_unix) afterwards to
    /// synchronize to the actual current time.
    ///
    /// See [`ClockTime`] for a complete example.
    ///
    /// # Arguments
    ///
    /// * `initial_offset_minutes` - UTC offset in minutes (e.g., -480 for PST/UTC-8)
    pub fn new(initial_offset_minutes: i32) -> Self {
        info!("Now: {:?}", Instant::now());
        Self {
            offset: Duration::from_millis(12 * 3600 * 1000),
            offset_minutes: initial_offset_minutes,
        }
    }

    /// Get the current UTC offset in minutes.
    ///
    /// See [`ClockTime`] for a complete example.
    #[must_use]
    pub const fn offset_minutes(&self) -> i32 {
        self.offset_minutes
    }

    /// Synchronize the clock to a Unix timestamp.
    ///
    /// Converts the provided UTC Unix timestamp to local time using the configured
    /// UTC offset, then adjusts the internal offset so future calls to
    /// [`h_m_s_sleep_duration()`](ClockTime::h_m_s_sleep_duration) return the correct time.
    ///
    /// See [`ClockTime`] for a complete example.
    ///
    /// # Arguments
    ///
    /// * `unix_seconds` - UTC Unix timestamp to synchronize to
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "Modulo operations prevent overflow"
    )]
    pub fn set_from_unix(&mut self, unix_seconds: UnixSeconds) {
        let local_seconds = unix_seconds.as_i64() + i64::from(self.offset_minutes) * 60;
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

    /// Get hours, minutes, seconds, and duration until next unit boundary.
    ///
    /// Returns the current time in 12-hour format along with the duration to sleep
    /// until the next boundary of the specified unit (typically [`ONE_MINUTE`] or [`ONE_SECOND`]).
    ///
    /// See [`ClockTime`] for a complete example.
    ///
    /// # Arguments
    ///
    /// * `unit` - Time unit boundary to calculate sleep duration for
    ///
    /// # Returns
    ///
    /// Tuple of `(hours, minutes, seconds, sleep_duration)` where:
    /// - `hours` is in 12-hour format (1-12)
    /// - `minutes` and `seconds` are 0-59
    /// - `sleep_duration` is time until next unit boundary
    #[expect(
        clippy::cast_possible_truncation,
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "Modulo prevents overflow"
    )]
    #[must_use]
    #[inline]
    pub fn h_m_s_sleep_duration(&self, unit: Duration) -> (u8, u8, u8, Duration) {
        let now = self.now();
        let sleep_duration = Self::till_next(now, unit);
        let elapsed_seconds = now.as_secs();
        let hours = ((elapsed_seconds / 3600) + 11) % 12 + 1;
        let minutes = (elapsed_seconds % 3600) / 60;
        let seconds = elapsed_seconds % 60;
        (hours as u8, minutes as u8, seconds as u8, sleep_duration)
    }

    /// Set the UTC offset in minutes.
    ///
    /// Useful for daylight saving time changes or when the user travels to a different
    /// timezone. The displayed time is adjusted accordingly to maintain continuity.
    ///
    /// See [`ClockTime`] for a complete example.
    ///
    /// # Arguments
    ///
    /// * `offset_minutes` - New UTC offset in minutes (e.g., -420 for PDT/UTC-7)
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Delta calculation is safe for i32 range"
    )]
    pub fn set_offset_minutes(&mut self, offset_minutes: i32) {
        let delta_minutes = offset_minutes - self.offset_minutes;
        let delta_seconds = delta_minutes * 60;

        if delta_seconds >= 0 {
            self.offset += Duration::from_secs(delta_seconds as u64);
        } else {
            self.offset -= Duration::from_secs((-delta_seconds) as u64);
        }

        self.offset_minutes = offset_minutes;
        info!(
            "Set UTC offset to {} minutes (delta: {} minutes)",
            offset_minutes, delta_minutes
        );
    }

    /// Get the current time of day.
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::integer_division_remainder_used,
        reason = "Modulo prevents overflow"
    )]
    #[inline]
    #[must_use]
    fn now(&self) -> Duration {
        let ticks = Instant::now().as_ticks() % TICKS_IN_ONE_DAY
            + self.offset.as_ticks() % TICKS_IN_ONE_DAY;
        Duration::from_ticks(ticks % TICKS_IN_ONE_DAY)
    }

    /// Calculate duration until next unit boundary.
    #[inline]
    #[must_use]
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "Modulo prevents overflow"
    )]
    const fn till_next(time: Duration, unit: Duration) -> Duration {
        let unit_ticks = unit.as_ticks();
        Duration::from_ticks(unit_ticks - time.as_ticks() % unit_ticks)
    }
}
