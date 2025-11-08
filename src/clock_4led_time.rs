use core::ops::AddAssign;
use core::sync::atomic::{AtomicI32, Ordering};

use defmt::info;
use embassy_time::{Duration, Instant};

use crate::clock_4led_constants::TICKS_IN_ONE_DAY;
use crate::unix_seconds::UnixSeconds;

static INITIAL_UTC_OFFSET_MINUTES: AtomicI32 = AtomicI32::new(0);
static CURRENT_UTC_OFFSET_MINUTES: AtomicI32 = AtomicI32::new(0);

pub struct ClockTime {
    offset: Duration,
    utc_offset_minutes: i32,
}

impl Default for ClockTime {
    fn default() -> Self {
        info!("Now: {:?}", Instant::now());
        let utc_offset_minutes = INITIAL_UTC_OFFSET_MINUTES.load(Ordering::Relaxed);
        CURRENT_UTC_OFFSET_MINUTES.store(utc_offset_minutes, Ordering::Relaxed);
        Self {
            offset: Duration::from_millis(12 * 3600 * 1000),
            utc_offset_minutes,
        }
    }
}

#[must_use]
pub fn current_utc_offset_minutes() -> i32 {
    CURRENT_UTC_OFFSET_MINUTES.load(Ordering::Relaxed)
}

pub fn set_initial_utc_offset_minutes(minutes: i32) {
    INITIAL_UTC_OFFSET_MINUTES.store(minutes, Ordering::Relaxed);
    CURRENT_UTC_OFFSET_MINUTES.store(minutes, Ordering::Relaxed);
}

impl ClockTime {
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

    #[expect(
        clippy::integer_division_remainder_used,
        reason = "Division converts minutes to hours"
    )]
    #[must_use]
    pub fn utc_offset_hours(&self) -> i32 {
        if self.utc_offset_minutes >= 0 {
            (self.utc_offset_minutes + 30) / 60
        } else {
            (self.utc_offset_minutes - 30) / 60
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        clippy::integer_division_remainder_used,
        reason = "Wrapping arithmetic is intentional"
    )]
    pub fn adjust_utc_offset_hours(&mut self, hours: i32) {
        let current_offset_hours = self.utc_offset_hours();
        let new_offset_hours = current_offset_hours + hours;
        let wrapped = ((new_offset_hours + 12) % 27 + 27) % 27 - 12;
        let delta_hours = wrapped - current_offset_hours;

        if delta_hours >= 0 {
            self.offset += Duration::from_secs((delta_hours * 3600) as u64);
        } else {
            self.offset -= Duration::from_secs(((-delta_hours) * 3600) as u64);
        }

        self.utc_offset_minutes = wrapped * 60;
        CURRENT_UTC_OFFSET_MINUTES.store(self.utc_offset_minutes, Ordering::Relaxed);
        info!(
            "Adjusted UTC offset from {} to {} hours (delta: {} hours)",
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
