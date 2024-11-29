use core::ops::AddAssign;

use defmt::info;
use embassy_time::{Duration, Instant};

use crate::TICKS_IN_ONE_DAY;

/// The system time along with an offset to represent time
/// to display on the clock.
pub struct ClockTime {
    offset: Duration,
}

impl Default for ClockTime {
    /// By default, `ClockTime` is born holding the time of the last build. If
    /// the build time is not available, it starts at Midnight.
    ///
    /// The build time is set by the `build.rs` script which sets the `BUILD_TIME`
    /// environment variable to the number of milliseconds since the Unix epoch.
    fn default() -> Self {
        let build_time_millis = option_env!("BUILD_TIME")
            .and_then(|val| val.parse::<u64>().ok())
            .unwrap_or(0);
        info!("Now: {:?}", Instant::now());
        // Convert build time (Unix epoch) to an offset Duration
        Self {
            offset: Duration::from_millis(build_time_millis),
        }
    }
}

impl ClockTime {
    /// Returns the current time with the offset applied wrapped around to be less than one day.
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::integer_division_remainder_used,
        reason = "Because of %'s will never overflow."
    )]
    #[inline]
    #[must_use]
    pub fn now(&self) -> Duration {
        let ticks = Instant::now().as_ticks() % TICKS_IN_ONE_DAY
            + self.offset.as_ticks() % TICKS_IN_ONE_DAY;
        Duration::from_ticks(ticks % TICKS_IN_ONE_DAY)
    }

    /// Returns the current hours, minutes, seconds, and wait duration until the next unit of time.
    ///
    /// For example, if `unit` is `ONE_MINUTE`, this function will tell how long to wait
    /// until the top of the next minute. This is used to put the microcontroller to sleep
    /// until the next time the display needs to be updated.
    ///
    /// The function is in-line so that the compiler can optimize return values that
    /// are not used.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "The modulo operations prevent overflow."
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
        reason = "The modulo operations prevent overflow."
    )]
    /// Returns the duration until the next unit of time.
    ///
    /// For example, if `a` is 1:02:03 and `unit` is `ONE_HOUR`, this function will return
    /// the duration until 2:00:00 which is 57 minutes and 57 seconds.
    pub const fn till_next(time: Duration, unit: Duration) -> Duration {
        let unit_ticks = unit.as_ticks();
        Duration::from_ticks(unit_ticks - time.as_ticks() % unit_ticks)
    }
}

impl AddAssign<Duration> for ClockTime {
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::arithmetic_side_effects,
        reason = "The modulo operations prevent overflow."
    )]
    /// Adds the given duration to offset, wrapping around to be less than one day.
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
