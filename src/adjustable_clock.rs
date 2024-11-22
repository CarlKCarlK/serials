use core::ops::AddAssign;

use embassy_time::{Duration, Instant};

pub struct AdjustableClock {
    start: Instant,
    offset: Duration,
}

impl Default for AdjustableClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            offset: Duration::default(),
        }
    }
}

impl AdjustableClock {
    #[inline]
    fn now(&self) -> Duration {
        Instant::now() - self.start + self.offset
    }

    // If only one or two of the components (e.g., hours or minutes) are used, the compiler can eliminate the unused calculations during inlining
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    pub fn h_m_s(&self) -> (u8, u8, u8) {
        let elapsed_seconds = self.now().as_secs();
        let hours = ((elapsed_seconds / 3600) + 11) % 12 + 1; // 1-12 instead of 0-11
        let minutes = (elapsed_seconds % 3600) / 60;
        let seconds = elapsed_seconds % 60;
        (hours as u8, minutes as u8, seconds as u8)
    }
}

impl AddAssign<Duration> for AdjustableClock {
    fn add_assign(&mut self, duration: Duration) {
        self.offset += duration;
    }
}
