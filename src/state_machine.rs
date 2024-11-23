use crate::{
    button::{Button, PressDuration},
    virtual_clock::{BlinkMode, ClockMode, VirtualClock},
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
    pub(crate) async fn next_state(
        self,
        virtual_clock: &mut VirtualClock,
        button: &mut Button,
    ) -> State {
        match self {
            State::First => State::DisplayHoursMinutes,
            State::DisplayHoursMinutes => display_hours_minutes_state(virtual_clock, button).await,
            State::DisplayMinutesSeconds => {
                display_minutes_seconds_state(virtual_clock, button).await
            }
            State::ShowSeconds => show_seconds_state(virtual_clock, button).await,
            State::EditSeconds => edit_seconds_state(virtual_clock, button).await,
            State::ShowMinutes => show_minutes_state(virtual_clock, button).await,
            State::EditMinutes => edit_minutes_state(virtual_clock, button).await,
            State::ShowHours => show_hours_state(virtual_clock, button).await,
            State::EditHours => edit_hours_state(virtual_clock, button).await,
            State::Last => State::First,
        }
    }
}

async fn display_hours_minutes_state(
    virtual_clock: &mut VirtualClock,
    button: &mut Button,
) -> State {
    virtual_clock
        .set_mode(ClockMode::HhMm, BlinkMode::NoBlink)
        .await;
    match button.wait_for_press().await {
        PressDuration::Short => State::DisplayMinutesSeconds,
        PressDuration::Long => State::ShowSeconds,
    }
}

async fn display_minutes_seconds_state(
    virtual_clock: &mut VirtualClock,
    button: &mut Button,
) -> State {
    virtual_clock
        .set_mode(ClockMode::MmSs, BlinkMode::NoBlink)
        .await;
    match button.wait_for_press().await {
        PressDuration::Short => State::DisplayHoursMinutes,
        PressDuration::Long => State::ShowSeconds,
    }
}

async fn show_seconds_state(virtual_clock: &mut VirtualClock, button: &mut Button) -> State {
    virtual_clock
        .set_mode(ClockMode::Ss, BlinkMode::BlinkingAndOn)
        .await;
    button.wait_for_up().await;
    match button.wait_for_press().await {
        PressDuration::Short => State::ShowMinutes,
        PressDuration::Long => State::EditSeconds,
    }
}

async fn edit_seconds_state(virtual_clock: &mut VirtualClock, button: &mut Button) -> State {
    virtual_clock
        .set_mode(ClockMode::SsIs00, BlinkMode::NoBlink)
        .await;
    button.inner.wait_for_rising_edge().await; // cmk raising edge?
    virtual_clock.reset_seconds().await;
    State::ShowSeconds
}

async fn show_minutes_state(virtual_clock: &mut VirtualClock, button: &mut Button) -> State {
    virtual_clock
        .set_mode(ClockMode::Mm, BlinkMode::BlinkingAndOn)
        .await;
    match button.wait_for_press().await {
        PressDuration::Short => State::ShowHours,
        PressDuration::Long => State::EditMinutes,
    }
}

async fn edit_minutes_state(virtual_clock: &mut VirtualClock, button: &mut Button) -> State {
    loop {
        if let Either::Second(()) = select(
            Timer::after(Duration::from_millis(250)),
            button.inner.wait_for_rising_edge(),
        )
        .await
        {
            return State::ShowMinutes;
        }
        virtual_clock.adjust_offset(ONE_MINUTE).await;
        virtual_clock
            .set_mode(ClockMode::Mm, BlinkMode::NoBlink)
            .await;
    }
}

async fn show_hours_state(virtual_clock: &mut VirtualClock, button: &mut Button) -> State {
    virtual_clock
        .set_mode(ClockMode::Hh, BlinkMode::BlinkingAndOn)
        .await;
    match button.wait_for_press().await {
        PressDuration::Short => State::Last,
        PressDuration::Long => State::EditHours,
    }
}

async fn edit_hours_state(virtual_clock: &mut VirtualClock, button: &mut Button) -> State {
    loop {
        if let Either::Second(()) = select(
            Timer::after(Duration::from_millis(500)),
            button.inner.wait_for_rising_edge(),
        )
        .await
        {
            return State::ShowHours;
        }
        virtual_clock.adjust_offset(ONE_HOUR).await;
        virtual_clock
            .set_mode(ClockMode::Hh, BlinkMode::NoBlink)
            .await;
    }
}

// cmk make a method?
#[inline]
pub fn tens_digit(value: u8) -> char {
    ((value / 10) + b'0') as char
}

#[inline]
pub fn tens_hours(value: u8) -> char {
    if value >= 10 {
        '1'
    } else {
        ' '
    }
}

#[inline]
pub fn ones_digit(value: u8) -> char {
    ((value % 10) + b'0') as char
}

// cmk move
pub const ONE_MINUTE: Duration = Duration::from_secs(60);
pub const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
