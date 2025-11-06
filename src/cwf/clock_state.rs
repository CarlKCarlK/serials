use crate::button::{Button, PressDuration};
use crate::cwf::clock::Clock;
use crate::cwf::time_sync::{TimeSync, TimeSyncEvent};
use crate::cwf::{BlinkState, ClockTime, ONE_MINUTE, ONE_SECOND};
use defmt::info;
use embassy_futures::select::{Either, select};
use embassy_time::Duration;

#[derive(Debug, defmt::Format, Clone, Copy, Default)]
pub enum ClockState {
    #[default]
    HoursMinutes,
    Connecting,
    MinutesSeconds,
    EditUtcOffset {
        modified: bool,
    },
    ConfirmClear(ConfirmClearChoice),
    ConfirmedClear,
    ClearingDone,
    AccessPointSetup,
}

#[derive(Debug, defmt::Format, Clone, Copy, Eq, PartialEq)]
pub enum ConfirmClearChoice {
    Keep,
    Clear,
}

impl ConfirmClearChoice {
    const fn toggle(self) -> Self {
        match self {
            Self::Keep => Self::Clear,
            Self::Clear => Self::Keep,
        }
    }
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
            Self::Connecting => self.execute_connecting(clock, time_sync).await,
            Self::MinutesSeconds => self.execute_minutes_seconds(clock, button, time_sync).await,
            Self::EditUtcOffset { modified } => {
                self.execute_edit_utc_offset(clock, button, modified).await
            }
            Self::ConfirmClear(selection) => {
                self.execute_confirm_clear(clock, button, selection).await
            }
            Self::ConfirmedClear => self.execute_confirmed_clear(clock).await,
            Self::ClearingDone => self.execute_clearing_done(clock).await,
            Self::AccessPointSetup => self.execute_access_point_setup(clock).await,
        }
    }

    pub(crate) fn render(self, clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        match self {
            Self::HoursMinutes => Self::render_hours_minutes(clock_time),
            Self::Connecting => Self::render_connecting(clock_time),
            Self::MinutesSeconds => Self::render_minutes_seconds(clock_time),
            Self::EditUtcOffset { .. } => Self::render_edit_utc_offset(clock_time),
            Self::ConfirmClear(selection) => Self::render_confirm_clear(selection),
            Self::ConfirmedClear => Self::render_confirmed_clear(),
            Self::ClearingDone => Self::render_clearing_done(),
            Self::AccessPointSetup => Self::render_access_point_setup(),
        }
    }

    async fn execute_connecting(self, clock: &Clock<'_>, time_sync: &TimeSync) -> Self {
        clock.set_state(self).await;
        let event = time_sync.wait().await;
        let success = matches!(event, TimeSyncEvent::Success { .. });
        Self::handle_time_sync_event(clock, event).await;
        if success { Self::HoursMinutes } else { self }
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
            Either::First(PressDuration::Long) => Self::EditUtcOffset { modified: false },
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
            Either::First(PressDuration::Long) => Self::EditUtcOffset { modified: false },
            Either::Second(event) => {
                Self::handle_time_sync_event(clock, event).await;
                self
            }
        }
    }

    async fn execute_edit_utc_offset(
        self,
        clock: &Clock<'_>,
        button: &mut Button<'_>,
        modified: bool,
    ) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => {
                clock.adjust_utc_offset_hours(1).await;
                let next_state = Self::EditUtcOffset { modified: true };
                clock.set_state(next_state).await;
                next_state
            }
            PressDuration::Long => {
                if modified {
                    Self::HoursMinutes
                } else {
                    Self::ConfirmClear(ConfirmClearChoice::Keep)
                }
            }
        }
    }

    async fn execute_confirm_clear(
        self,
        clock: &Clock<'_>,
        button: &mut Button<'_>,
        selection: ConfirmClearChoice,
    ) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => Self::ConfirmClear(selection.toggle()),
            PressDuration::Long => match selection {
                ConfirmClearChoice::Keep => Self::HoursMinutes,
                ConfirmClearChoice::Clear => Self::ConfirmedClear,
            },
        }
    }

    async fn execute_confirmed_clear(self, clock: &Clock<'_>) -> Self {
        clock.set_state(self).await;
        self
    }

    async fn execute_clearing_done(self, clock: &Clock<'_>) -> Self {
        clock.set_state(self).await;
        self
    }

    async fn execute_access_point_setup(self, clock: &Clock<'_>) -> Self {
        clock.set_state(self).await;
        Self::HoursMinutes
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

    fn render_confirm_clear(selection: ConfirmClearChoice) -> (BlinkState, [char; 4], Duration) {
        match selection {
            ConfirmClearChoice::Keep => (
                BlinkState::Solid,
                ['-', '-', '-', '-'],
                Duration::from_millis(400),
            ),
            ConfirmClearChoice::Clear => (
                BlinkState::BlinkingAndOn,
                ['C', 'L', 'r', ' '],
                Duration::from_millis(400),
            ),
        }
    }

    fn render_confirmed_clear() -> (BlinkState, [char; 4], Duration) {
        (
            BlinkState::BlinkingAndOn,
            ['C', 'L', 'r', ' '],
            Duration::from_millis(400),
        )
    }

    fn render_clearing_done() -> (BlinkState, [char; 4], Duration) {
        (
            BlinkState::Solid,
            ['D', 'O', 'N', 'E'],
            Duration::from_millis(600),
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
