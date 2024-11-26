use crate::{
    button::{Button, PressDuration},
    clock::{Clock, ClockMode},
};
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Timer};

#[derive(Debug, defmt::Format)]
pub(crate) enum State {
    First,
    DisplayHoursMinutes,
    DisplayMinutesSeconds,
    ShowSeconds,
    EditSeconds,
    ShowMinutes,
    EditMinutes,
    ShowHours,
    EditHours,
    Last,
}

impl Default for State {
    fn default() -> Self {
        Self::First
    }
}
impl State {
    pub(crate) async fn next_state(self, clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        match self {
            State::First => State::DisplayHoursMinutes,
            State::DisplayHoursMinutes => State::display_hours_minutes(clock, button).await,
            State::DisplayMinutesSeconds => State::display_minutes_seconds(clock, button).await,
            State::ShowSeconds => State::show_seconds(clock, button).await,
            State::EditSeconds => State::edit_seconds(clock, button).await,
            State::ShowMinutes => State::show_minutes(clock, button).await,
            State::EditMinutes => State::edit_minutes(clock, button).await,
            State::ShowHours => State::show_hours(clock, button).await,
            State::EditHours => State::edit_hours(clock, button).await,
            State::Last => State::First,
        }
    }

    async fn display_hours_minutes(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        clock.set_mode(ClockMode::HoursMinutes).await;
        match button.press_duration().await {
            PressDuration::Short => State::DisplayMinutesSeconds,
            PressDuration::Long => State::ShowSeconds,
        }
    }

    async fn display_minutes_seconds(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        clock.set_mode(ClockMode::MinutesSeconds).await;
        match button.press_duration().await {
            PressDuration::Short => State::DisplayHoursMinutes,
            PressDuration::Long => State::ShowSeconds,
        }
    }

    async fn show_seconds(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        clock.set_mode(ClockMode::BlinkingSeconds).await;
        button.wait_for_up().await;
        match button.press_duration().await {
            PressDuration::Short => State::ShowMinutes,
            PressDuration::Long => State::EditSeconds,
        }
    }

    async fn edit_seconds(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        clock.set_mode(ClockMode::SecondsZero).await;
        button.wait_for_press().await;
        clock.reset_seconds().await;
        State::ShowSeconds
    }

    async fn show_minutes(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        clock.set_mode(ClockMode::BlinkingMinutes).await;
        match button.press_duration().await {
            PressDuration::Short => State::ShowHours,
            PressDuration::Long => State::EditMinutes,
        }
    }

    async fn edit_minutes(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        loop {
            if let Either::Second(_) = select(
                Timer::after(Duration::from_millis(250)),
                button.wait_for_press(),
            )
            .await
            {
                return State::ShowMinutes;
            }
            clock.adjust_offset(ONE_MINUTE).await;
            clock.set_mode(ClockMode::SolidMinutes).await;
        }
    }

    async fn show_hours(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        clock.set_mode(ClockMode::BlinkingHours).await;
        match button.press_duration().await {
            PressDuration::Short => State::Last,
            PressDuration::Long => State::EditHours,
        }
    }

    async fn edit_hours(clock: &mut Clock<'_>, button: &mut Button<'_>) -> State {
        loop {
            if let Either::Second(_) = select(
                Timer::after(Duration::from_millis(500)),
                button.wait_for_press(),
            )
            .await
            {
                return State::ShowHours;
            }
            clock.adjust_offset(ONE_HOUR).await;
            clock.set_mode(ClockMode::SolidHours).await;
        }
    }
}
// cmk move
pub const ONE_MINUTE: Duration = Duration::from_secs(60);
pub const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
