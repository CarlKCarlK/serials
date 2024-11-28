use crate::{
    button::{Button, PressDuration},
    clock::Clock,
    shared_constants::{HOUR_EDIT_SPEED, MINUTE_EDIT_SPEED, ONE_HOUR, ONE_MINUTE},
};
use embassy_futures::select::{select, Either};
use embassy_time::Timer;

// cmk can/should these be merged in with ClockState?
#[derive(Debug, defmt::Format)]
/// Represents the different states of the clock's user interaction.
pub enum ClockState {
    DisplayHoursMinutes,
    DisplayMinutesSeconds,
    ShowSeconds,
    EditSeconds,
    ShowMinutes,
    EditMinutes,
    ShowHours,
    EditHours,
}

impl Default for ClockState {
    fn default() -> Self {
        Self::DisplayHoursMinutes
    }
}
impl ClockState {
    pub async fn next_state(self, clock: &mut Clock<'_>, button: &mut Button<'_>) -> Self {
        match self {
            Self::DisplayHoursMinutes => Self::display_hours_minutes(clock, button).await,
            Self::DisplayMinutesSeconds => Self::display_minutes_seconds(clock, button).await,
            Self::ShowSeconds => Self::show_seconds(clock, button).await,
            Self::EditSeconds => Self::edit_seconds(clock, button).await,
            Self::ShowMinutes => Self::show_minutes(clock, button).await,
            Self::EditMinutes => Self::edit_minutes(clock, button).await,
            Self::ShowHours => Self::show_hours(clock, button).await,
            Self::EditHours => Self::edit_hours(clock, button).await,
        }
    }

    async fn display_hours_minutes(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_mode(Self::DisplayHoursMinutes).await;
        match button.press_duration().await {
            PressDuration::Short => Self::DisplayMinutesSeconds,
            PressDuration::Long => Self::ShowSeconds,
        }
    }

    async fn display_minutes_seconds(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_mode(Self::DisplayMinutesSeconds).await;
        match button.press_duration().await {
            PressDuration::Short => Self::DisplayHoursMinutes,
            PressDuration::Long => Self::ShowSeconds,
        }
    }

    async fn show_seconds(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_mode(Self::ShowSeconds).await;
        button.wait_for_up().await;
        match button.press_duration().await {
            PressDuration::Short => Self::ShowMinutes,
            PressDuration::Long => Self::EditSeconds,
        }
    }

    async fn edit_seconds(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_mode(ClockState::EditSeconds).await;
        button.wait_for_press().await;
        clock.reset_seconds().await;
        Self::ShowSeconds
    }

    async fn show_minutes(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_mode(Self::ShowMinutes).await;
        match button.press_duration().await {
            PressDuration::Short => Self::ShowHours,
            PressDuration::Long => Self::EditMinutes,
        }
    }

    async fn edit_minutes(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        loop {
            if let Either::Second(_) =
                select(Timer::after(MINUTE_EDIT_SPEED), button.wait_for_press()).await
            {
                return Self::ShowMinutes;
            }
            clock.adjust_offset(ONE_MINUTE).await;
            clock.set_mode(Self::EditMinutes).await;
        }
    }

    async fn show_hours(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        clock.set_mode(Self::ShowHours).await;
        match button.press_duration().await {
            PressDuration::Short => Self::DisplayHoursMinutes,
            PressDuration::Long => Self::EditHours,
        }
    }

    async fn edit_hours(clock: &Clock<'_>, button: &mut Button<'_>) -> Self {
        loop {
            if let Either::Second(_) =
                select(Timer::after(HOUR_EDIT_SPEED), button.wait_for_press()).await
            {
                return Self::ShowHours;
            }
            clock.adjust_offset(ONE_HOUR).await;
            clock.set_mode(Self::EditHours).await;
        }
    }
}
