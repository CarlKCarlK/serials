//! Unix timestamp type for time-related devices

use defmt::Format;
use time::{OffsetDateTime, UtcOffset};

/// Units-safe wrapper for Unix timestamps (seconds since 1970-01-01 00:00:00 UTC)
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Format)]
pub struct UnixSeconds(pub i64);

impl UnixSeconds {
    /// Get the underlying i64 value
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    /// Convert NTP seconds (since 1900-01-01) to Unix seconds (since 1970-01-01)
    #[must_use]
    pub const fn from_ntp_seconds(ntp: u32) -> Option<Self> {
        // 1900→1970 offset: 70 years * 365.25 days/year * 86400 seconds/day
        const NTP_TO_UNIX_SECONDS: i64 = 2_208_988_800;
        // Promote to i64 safely, then subtract
        let s = (ntp as i64) - NTP_TO_UNIX_SECONDS;
        // Reject negative (pre-1970)
        if s >= 0 { Some(Self(s)) } else { None }
    }

    /// Convert to OffsetDateTime with the given timezone offset
    #[must_use]
    pub fn to_offset_datetime(self, offset: UtcOffset) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(self.as_i64())
            .ok()
            .map(|dt| dt.to_offset(offset))
    }
}
