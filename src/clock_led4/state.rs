//! State machine for 4-digit LED clock display modes and transitions.

use super::time::ClockTime;
use super::time::{ONE_MINUTE, ONE_SECOND};
use crate::button::{Button, PressDuration};
use crate::clock_led4::ClockLed4 as Clock;
use crate::led4::BlinkState;
use crate::time_sync::{TimeSync, TimeSyncEvent};
use defmt::info;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant};

/// Display states for the 4-digit LED clock.
#[derive(Debug, defmt::Format, Clone, Copy, Default)]
pub enum ClockLed4State {
    #[default]
    HoursMinutes,
    Connecting,
    MinutesSeconds,
    EditUtcOffset,
    AccessPointSetup,
}

impl ClockLed4State {
    /// Execute the state machine for this clock state.
    pub async fn execute(
        self,
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        match self {
            Self::HoursMinutes => self.execute_hours_minutes(clock, button, time_sync).await,
            Self::Connecting => self.execute_connecting(clock, time_sync).await,
            Self::MinutesSeconds => self.execute_minutes_seconds(clock, button, time_sync).await,
            Self::EditUtcOffset => self.execute_edit_utc_offset(clock, button).await,
            Self::AccessPointSetup => self.execute_access_point_setup(clock, time_sync).await,
        }
    }

    /// Render the current clock state to display output.
    pub fn render(self, clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        match self {
            Self::HoursMinutes => Self::render_hours_minutes(clock_time),
            Self::Connecting => Self::render_connecting(clock_time),
            Self::MinutesSeconds => Self::render_minutes_seconds(clock_time),
            Self::EditUtcOffset => Self::render_edit_utc_offset(clock_time),
            Self::AccessPointSetup => Self::render_access_point_setup(),
        }
    }

    async fn execute_connecting(self, clock: &Clock<'_>, time_sync: &TimeSync) -> Self {
        clock.set_state(self).await;
        let deadline_ticks = Instant::now()
            .as_ticks()
            .saturating_add(ONE_MINUTE.as_ticks());

        loop {
            let now_ticks = Instant::now().as_ticks();
            if now_ticks >= deadline_ticks {
                return Self::AccessPointSetup;
            }

            let remaining_ticks = deadline_ticks - now_ticks;
            if remaining_ticks == 0 {
                return Self::AccessPointSetup;
            }

            let timeout = Duration::from_ticks(remaining_ticks);
            match embassy_time::with_timeout(timeout, time_sync.wait()).await {
                Ok(event) => match event {
                    success @ TimeSyncEvent::Success { .. } => {
                        Self::handle_time_sync_event(clock, success).await;
                        return Self::HoursMinutes;
                    }
                    failure @ TimeSyncEvent::Failed(_) => {
                        Self::handle_time_sync_event(clock, failure).await;
                    }
                },
                Err(_) => return Self::AccessPointSetup,
            }
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
                clock.set_state(Self::EditUtcOffset).await;
                Self::EditUtcOffset
            }
            PressDuration::Long => Self::HoursMinutes,
        }
    }

    async fn execute_access_point_setup(self, clock: &Clock<'_>, time_sync: &TimeSync) -> Self {
        clock.set_state(self).await;
        loop {
            match time_sync.wait().await {
                success @ TimeSyncEvent::Success { .. } => {
                    Self::handle_time_sync_event(clock, success).await;
                    return Self::HoursMinutes;
                }
                failure @ TimeSyncEvent::Failed(_) => {
                    Self::handle_time_sync_event(clock, failure).await;
                }
            }
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

    fn render_connecting(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        const FRAME_DURATION: Duration = Duration::from_millis(120);
        const TOP: char = '\'';
        const TOP_RIGHT: char = '"';
        const RIGHT: char = '>';
        const BOTTOM_RIGHT: char = ')';
        const BOTTOM: char = '_';
        const BOTTOM_LEFT: char = '*';
        const LEFT: char = '<';
        const TOP_LEFT: char = '(';
        const FRAMES: [[char; 4]; 8] = [
            [TOP, TOP, TOP, TOP],
            [TOP, TOP, TOP, TOP_RIGHT],
            [' ', ' ', ' ', RIGHT],
            [' ', ' ', ' ', BOTTOM_RIGHT],
            [BOTTOM, BOTTOM, BOTTOM, BOTTOM],
            [BOTTOM_LEFT, BOTTOM, BOTTOM, BOTTOM],
            [LEFT, ' ', ' ', ' '],
            [TOP_LEFT, TOP, TOP, TOP],
        ];

        let frame_duration_ticks = FRAME_DURATION.as_ticks();
        let frame_index = if frame_duration_ticks == 0 {
            0
        } else {
            let now_ticks = clock_time.now().as_ticks();
            ((now_ticks / frame_duration_ticks) % FRAMES.len() as u64) as usize
        };

        (BlinkState::Solid, FRAMES[frame_index], FRAME_DURATION)
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

    fn render_access_point_setup() -> (BlinkState, [char; 4], Duration) {
        (
            BlinkState::BlinkingAndOn,
            ['C', 'O', 'n', 'n'],
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
