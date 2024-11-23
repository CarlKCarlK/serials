use core::ops::AddAssign;

use defmt::info;
use embassy_time::{Duration, Instant};

pub struct OffsetTime {
    offset: Duration,
}

impl Default for OffsetTime {
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
    pub fn h_m_s_update(&self, unit: Duration) -> (u8, u8, u8, Duration) {
        let now = self.now();
        let unit_ticks = unit.as_ticks();
        let update = Duration::from_ticks(unit_ticks - (now.as_ticks() % unit_ticks));
        let elapsed_seconds = now.as_secs();
        let hours = ((elapsed_seconds / 3600) + 11) % 12 + 1; // 1-12 instead of 0-11
        let minutes = (elapsed_seconds % 3600) / 60;
        let seconds = elapsed_seconds % 60;
        (hours as u8, minutes as u8, seconds as u8, update)
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
