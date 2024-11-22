use crate::{
    adjustable_clock::AdjustableClock,
    button::{Button, PressDuration},
    virtual_display::{self, VirtualDisplay, CELL_COUNT0},
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
        virtual_display: &mut virtual_display::VirtualDisplay<CELL_COUNT0>,
        button: &mut Button,
        adjustable_clock: &mut AdjustableClock,
    ) -> State {
        match self {
            State::First => State::DisplayHoursMinutes,
            State::DisplayHoursMinutes => {
                display_hours_minutes_state(virtual_display, button, adjustable_clock).await
            }
            State::DisplayMinutesSeconds => {
                display_minutes_seconds_state(virtual_display, button, adjustable_clock).await
            }
            State::ShowSeconds => {
                show_seconds_state(virtual_display, button, adjustable_clock).await
            }
            State::EditSeconds => {
                edit_seconds_state(virtual_display, button, adjustable_clock).await
            }
            State::ShowMinutes => {
                show_minutes_state(virtual_display, button, adjustable_clock).await
            }
            State::EditMinutes => {
                edit_minutes_state(virtual_display, button, adjustable_clock).await
            }
            State::ShowHours => show_hours_state(virtual_display, button, adjustable_clock).await,
            State::EditHours => edit_hours_state(virtual_display, button, adjustable_clock).await,
            State::Last => State::First,
        }
    }
}

const ONE_MIN: Duration = Duration::from_secs(60);
const ONE_HOUR: Duration = Duration::from_secs(60 * 60);

async fn display_hours_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &AdjustableClock,
) -> State {
    loop {
        let (hours, minutes, seconds) = adjustable_clock.h_m_s();

        virtual_display.write_chars([
            tens_digit(hours),
            ones_digit(hours),
            tens_digit(minutes),
            ones_digit(minutes),
        ]);

        // Sleep until the top of the next minute or until the button is pressed
        let till_next_minute = ONE_MIN - Duration::from_secs(seconds.into());
        if let Either::Second(press_duration) =
            select(Timer::after(till_next_minute), button.wait_for_press()).await
        {
            return match press_duration {
                PressDuration::Short => State::DisplayMinutesSeconds,
                PressDuration::Long => State::ShowSeconds,
            };
        }
    }
}

async fn display_minutes_seconds_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &AdjustableClock,
) -> State {
    loop {
        let (_, minutes, seconds) = adjustable_clock.h_m_s();

        virtual_display.write_chars([
            tens_digit(minutes),
            ones_digit(minutes),
            tens_digit(seconds),
            ones_digit(seconds),
        ]);

        if let Either::Second(press_duration) = select(
            Timer::after(Duration::from_secs(1)), // cmk const
            button.wait_for_press(),
        )
        .await
        {
            return match press_duration {
                PressDuration::Short => State::DisplayHoursMinutes,
                PressDuration::Long => State::ShowSeconds,
            };
        }
    }
}

async fn show_seconds_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &AdjustableClock,
) -> State {
    let (_, _, seconds) = adjustable_clock.h_m_s();

    virtual_display.write_chars([' ', tens_digit(seconds), ones_digit(seconds), ' ']);

    button.wait_for_up().await;
    match button.wait_for_press().await {
        PressDuration::Short => State::ShowMinutes,
        PressDuration::Long => State::EditSeconds,
    }
}

async fn edit_seconds_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &mut AdjustableClock,
) -> State {
    virtual_display.write_chars([' ', '0', '0', ' ']); // cmk blink this?
    button.inner.wait_for_rising_edge().await;
    let (_, _, seconds) = adjustable_clock.h_m_s();
    let till_next_minute = ONE_MIN - Duration::from_secs(seconds.into());
    *adjustable_clock += till_next_minute;
    State::ShowSeconds
}

async fn show_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &AdjustableClock,
) -> State {
    let (_, minutes, _) = adjustable_clock.h_m_s();

    virtual_display.write_chars([' ', ' ', tens_digit(minutes), ones_digit(minutes)]);

    match button.wait_for_press().await {
        PressDuration::Short => State::ShowHours,
        PressDuration::Long => State::EditMinutes,
    }
}

async fn edit_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &mut AdjustableClock,
) -> State {
    loop {
        if let Either::Second(()) = select(
            Timer::after(Duration::from_millis(250)),
            button.inner.wait_for_rising_edge(),
        )
        .await
        {
            return State::ShowMinutes;
        }
        *adjustable_clock += ONE_MIN;
        let (_, minutes, _) = adjustable_clock.h_m_s();

        virtual_display.write_chars([' ', ' ', tens_digit(minutes), ones_digit(minutes)]);
    }
}

async fn show_hours_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &AdjustableClock,
) -> State {
    let (hours, _, _) = adjustable_clock.h_m_s();
    virtual_display.write_chars([tens_hours(hours), ones_digit(hours), ' ', ' ']);

    match button.wait_for_press().await {
        PressDuration::Short => State::Last,
        PressDuration::Long => State::EditHours,
    }
}

async fn edit_hours_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut Button,
    adjustable_clock: &mut AdjustableClock,
) -> State {
    loop {
        if let Either::Second(()) = select(
            Timer::after(Duration::from_millis(500)),
            button.inner.wait_for_rising_edge(),
        )
        .await
        {
            return State::ShowHours;
        }
        *adjustable_clock += ONE_HOUR;

        let (hours, _, _) = adjustable_clock.h_m_s();
        virtual_display.write_chars([tens_hours(hours), ones_digit(hours), ' ', ' ']);
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
