use crate::{
    blinker::BlinkMode,
    button::{Button, PressDuration},
    clock::Clock,
    shared_constants::{HOUR_EDIT_SPEED, MINUTE_EDIT_SPEED, ONE_HOUR, ONE_MINUTE},
    ClockTime, ONE_DAY, ONE_SECOND,
};
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Timer};

/// Represents the different states the clock can operate in.
///
/// For example, `HoursMinutes` displays the hours and minutes and `ShowSeconds` blinks the seconds
/// to show that they are ready to be reset.
#[expect(missing_docs, reason = "The variants are self-explanatory.")]
#[derive(Debug, defmt::Format, Clone, Copy, Default)]
pub enum ClockState {
    #[default]
    HoursMinutes,
    MinutesSeconds,
    ShowSeconds,
    EditSeconds,
    ShowMinutes,
    EditMinutes,
    ShowHours,
    EditHours,
}

impl ClockState {
    /// Run the clock in the current state and return the next state.
    ///
    /// # Returns
    ///
    /// The next state of the clock.
    pub async fn run_and_next(self, clock: &mut Clock<'_>, button: &mut Button<'_>) -> Self {
        match self {
            Self::HoursMinutes => self.run_and_next_hours_minutes(clock, button).await,
            Self::MinutesSeconds => self.run_and_next_minutes_seconds(clock, button).await,
            Self::ShowSeconds => self.run_and_next_show_seconds(clock, button).await,
            Self::EditSeconds => self.run_and_next_edit_seconds(clock, button).await,
            Self::ShowMinutes => self.run_and_next_show_minutes(clock, button).await,
            Self::EditMinutes => self.run_and_next_edit_minutes(clock, button).await,
            Self::ShowHours => self.run_and_next_show_hours(clock, button).await,
            Self::EditHours => self.run_and_next_edit_hours(clock, button).await,
        }
    }

    /// Given the current `ClockMode` and `ClockTime`, generates the information the virtual `Clock` should display.
    ///
    /// # Example
    ///
    /// If the `ClockState` is `HoursMinutes` and the `ClockTime` is 1:23:45, the function will return:
    /// - Characters: `[' ', '1', '2', '3']`
    /// - Blink Mode: `BlinkMode::Solid`
    /// - Sleep Duration: `Duration::from_secs(15)`
    pub(crate) fn render(self, clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        match self {
            Self::HoursMinutes => Self::render_hours_minutes(clock_time),
            Self::MinutesSeconds => Self::render_minutes_seconds(clock_time),
            Self::ShowSeconds => Self::render_show_seconds(clock_time),
            Self::EditSeconds => Self::render_edit_seconds(clock_time),
            Self::ShowMinutes => Self::render_show_minutes(clock_time),
            Self::EditMinutes => Self::render_edit_minutes(clock_time),
            Self::ShowHours => Self::render_show_hours(clock_time),
            Self::EditHours => Self::render_edit_hours(clock_time),
        }
    }

    async fn run_and_next_hours_minutes(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => Self::MinutesSeconds,
            PressDuration::Long => Self::ShowSeconds,
        }
    }

    async fn run_and_next_minutes_seconds(
        self,
        clock: &Clock<'_>,
        button: &mut Button<'_>,
    ) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => Self::HoursMinutes,
            PressDuration::Long => Self::ShowSeconds,
        }
    }

    async fn run_and_next_show_seconds(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_state(self).await;
        button.wait_for_up().await;
        match button.press_duration().await {
            PressDuration::Short => Self::ShowMinutes,
            PressDuration::Long => Self::EditSeconds,
        }
    }

    async fn run_and_next_edit_seconds(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_state(self).await;
        button.wait_for_press().await;
        clock.reset_seconds().await;
        Self::ShowSeconds
    }

    async fn run_and_next_show_minutes(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => Self::ShowHours,
            PressDuration::Long => Self::EditMinutes,
        }
    }

    async fn run_and_next_edit_minutes(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        loop {
            if let Either::Second(_) =
                select(Timer::after(MINUTE_EDIT_SPEED), button.wait_for_press()).await
            {
                return Self::ShowMinutes;
            }
            clock.adjust_offset(ONE_MINUTE).await;
            clock.set_state(self).await;
        }
    }

    async fn run_and_next_show_hours(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => Self::HoursMinutes,
            PressDuration::Long => Self::EditHours,
        }
    }

    async fn run_and_next_edit_hours(self, clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        loop {
            if let Either::Second(_) =
                select(Timer::after(HOUR_EDIT_SPEED), button.wait_for_press()).await
            {
                return Self::ShowHours;
            }
            clock.adjust_offset(ONE_HOUR).await;
            clock.set_state(self).await;
        }
    }

    fn render_hours_minutes(clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        let (hours, minutes, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
            BlinkMode::Solid,
            sleep_duration,
        )
    }

    fn render_minutes_seconds(clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, minutes, seconds, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_SECOND);
        (
            [
                tens_digit(minutes),
                ones_digit(minutes),
                tens_digit(seconds),
                ones_digit(seconds),
            ],
            BlinkMode::Solid,
            sleep_duration,
        )
    }

    fn render_show_seconds(clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, _, seconds, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_SECOND);
        (
            [' ', tens_digit(seconds), ones_digit(seconds), ' '],
            BlinkMode::BlinkingAndOn,
            sleep_duration,
        )
    }

    const fn render_edit_seconds(_clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        // We don't really need to wake up even once a day to update
        // the constant "00" display, but Duration::MAX causes an overflow
        // so ONE_DAY is used instead.
        ([' ', '0', '0', ' '], BlinkMode::Solid, ONE_DAY)
    }

    fn render_show_minutes(clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, minutes, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            [' ', ' ', tens_digit(minutes), ones_digit(minutes)],
            BlinkMode::BlinkingAndOn,
            sleep_duration,
        )
    }

    fn render_edit_minutes(clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, minutes, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            [' ', ' ', tens_digit(minutes), ones_digit(minutes)],
            BlinkMode::Solid,
            sleep_duration,
        )
    }

    fn render_show_hours(clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        let (hours, _, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_HOUR);
        (
            [tens_hours(hours), ones_digit(hours), ' ', ' '],
            BlinkMode::BlinkingAndOn,
            sleep_duration,
        )
    }

    fn render_edit_hours(clock_time: &ClockTime) -> ([char; 4], BlinkMode, Duration) {
        let (hours, _, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_HOUR);
        (
            [tens_hours(hours), ones_digit(hours), ' ', ' '],
            BlinkMode::Solid,
            sleep_duration,
        )
    }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Because value < 60, the division is safe."
)]
const fn tens_digit(value: u8) -> char {
    debug_assert!(value < 60, "Value is between 0 and 59 (inclusive)");
    ((value / 10) + b'0') as char
}

#[inline]
const fn tens_hours(value: u8) -> char {
    debug_assert!(
        1 <= value && value <= 12,
        "Value is between 1 and 12 (inclusive)"
    );
    if value >= 10 {
        '1'
    } else {
        ' '
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Because value < 60, the division is safe."
)]
#[inline]
const fn ones_digit(value: u8) -> char {
    debug_assert!(value < 60, "Value is be between 0 and 59 (inclusive)");
    ((value % 10) + b'0') as char
}
