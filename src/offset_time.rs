use core::ops::AddAssign;

use defmt::info;
use embassy_time::{Duration, Instant};

/// The system time offset to represent time
/// to display on the clock.
pub struct OffsetTime {
    offset: Duration,
}

impl Default for OffsetTime {
    /// The default implementation of `OffsetTime` sets the offset to the build time.
    ///
    /// The build time is set by the `build.rs` script that generates the `Cargo.toml` file.
    /// It is represented as the number of milliseconds since the Unix epoch.
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

impl OffsetTime {
    #[inline]
    pub fn now(&self) -> Duration {
        Duration::from_ticks(Instant::now().as_ticks() + self.offset.as_ticks())
    }

    #[allow(clippy::cast_possible_truncation)]
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
    pub fn till_next(a: Duration, b: Duration) -> Duration {
        let b_ticks = b.as_ticks();
        Duration::from_ticks(b_ticks - a.as_ticks() % b_ticks)
    }
}

impl AddAssign<Duration> for OffsetTime {
    fn add_assign(&mut self, duration: Duration) {
        self.offset += duration;
        info!(
            "Now: {:?}, Offset: {:?}",
            Instant::now().as_millis(),
            self.offset.as_millis()
        );
    }
}
