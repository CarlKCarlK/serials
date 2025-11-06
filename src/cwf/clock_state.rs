use crate::button::{Button, PressDuration};
use crate::cwf::clock::Clock;
use crate::cwf::time_sync::{TimeSync, TimeSyncEvent};
use crate::cwf::{BlinkState, ClockTime, ONE_MINUTE, ONE_SECOND};
use defmt::info;
use embassy_futures::select::{select, Either};
use embassy_time::Duration;

#[derive(Debug, defmt::Format, Clone, Copy, Default)]
pub enum ClockState {
    #[default]
    HoursMinutes,
    MinutesSeconds,
    EditUtcOffset,
}

impl ClockState {
    pub async fn execute(
        self,
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        match self {
            Self::HoursMinutes => self.execute_hours_minutes(clock, button, time_sync).await,
            Self::MinutesSeconds => self.execute_minutes_seconds(clock, button, time_sync).await,
            Self::EditUtcOffset => self.execute_edit_utc_offset(clock, button).await,
        }
    }

    pub(crate) fn render(self, clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        match self {
            Self::HoursMinutes => Self::render_hours_minutes(clock_time),
            Self::MinutesSeconds => Self::render_minutes_seconds(clock_time),
            Self::EditUtcOffset => Self::render_edit_utc_offset(clock_time),
        }
    }

    async fn execute_hours_minutes(
        self,
        clock: &Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        clock.set_state(self).await;
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
        clock: &Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        clock.set_state(self).await;
        match select(button.press_duration(), time_sync.wait()).await {
            Either::First(PressDuration::Short) => Self::HoursMinutes,
            Either::First(PressDuration::Long) => Self::EditUtcOffset,
            Either::Second(event) => {
                Self::handle_time_sync_event(clock, event).await;
                self
            }
        }
    }

    async fn execute_edit_utc_offset(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => {
                clock.adjust_utc_offset_hours(1).await;
                clock.set_state(self).await;
                self
            }
            PressDuration::Long => Self::HoursMinutes,
        }
    }

    async fn handle_time_sync_event(clock: &Clock<'_>, event: TimeSyncEvent) {
        match event {
            TimeSyncEvent::Success { unix_seconds } => {
                info!(
                    "Time sync success: setting clock to {}",
                    unix_seconds.as_i64()
                );
                clock.set_time_from_unix(unix_seconds).await;
            }
            TimeSyncEvent::Failed(msg) => {
                info!("Time sync failed: {}", msg);
            }
        }
    }

    fn render_hours_minutes(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
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

    fn render_minutes_seconds(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
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

    fn render_edit_utc_offset(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
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
    reason = "Value < 60 ensures division is safe"
)]
const fn tens_digit(value: u8) -> char {
    ((value / 10) + b'0') as char
}

#[inline]
const fn tens_hours(value: u8) -> char {
    if value >= 10 { '1' } else { ' ' }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Value < 60 ensures division is safe"
)]
const fn ones_digit(value: u8) -> char {
    ((value % 10) + b'0') as char
}
