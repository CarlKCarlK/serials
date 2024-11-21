use core::ops::AddAssign;

use embassy_futures::select::{select, Either};
use embassy_rp::gpio;
use embassy_time::{Duration, Instant, Timer};

use crate::virtual_display::{self, VirtualDisplay, CELL_COUNT0};

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
    DisplayOff,
    Last,
}

pub(crate) async fn state_to_state(
    mut state: State,
    virtual_display: &mut virtual_display::VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &mut AdjustableClock,
) -> State {
    state = match state {
        State::First => State::DisplayHoursMinutes,
        State::DisplayHoursMinutes => {
            display_hours_minutes_state(virtual_display, button, adjustable_clock).await
        }
        State::DisplayMinutesSeconds => {
            display_minutes_seconds_state(virtual_display, button, adjustable_clock).await
        }
        State::ShowSeconds => show_seconds_state(virtual_display, button, adjustable_clock).await,
        State::EditSeconds => edit_seconds_state(virtual_display, button, adjustable_clock).await,
        State::ShowMinutes => show_minutes_state(virtual_display, button, adjustable_clock).await,
        State::EditMinutes => edit_minutes_state(virtual_display, button, adjustable_clock).await,
        State::ShowHours => show_hours_state(virtual_display, button, adjustable_clock).await,
        State::EditHours => edit_hours_state(virtual_display, button, adjustable_clock).await,
        State::DisplayOff => display_off_state(virtual_display, button, adjustable_clock).await,
        State::Last => State::First,
    };
    state
}

const ONE_MIN: Duration = Duration::from_secs(60);
const ONE_HOUR: Duration = Duration::from_secs(60 * 60);

async fn display_hours_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
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
        if let Either::Second(()) = select(
            Timer::after(till_next_minute),
            button.wait_for_rising_edge(),
        )
        .await
        {
            // cmk virtualize the button everywhere
            // cmk make the button work as soon as its held long enough
            button.wait_for_falling_edge().await;
            return State::DisplayMinutesSeconds;
        }
    }
}

#[inline]
fn tens_digit(value: u8) -> char {
    ((value / 10) + b'0') as char
}

#[inline]
fn ones_digit(value: u8) -> char {
    ((value % 10) + b'0') as char
}

async fn display_minutes_seconds_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
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

        if let Either::Second(()) = select(
            Timer::after(Duration::from_secs(1)),
            button.wait_for_rising_edge(),
        )
        .await
        {
            button.wait_for_falling_edge().await;
            return State::ShowSeconds;
        }
    }
}

async fn show_seconds_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &AdjustableClock,
) -> State {
    let (_, _, seconds) = adjustable_clock.h_m_s();

    virtual_display.write_chars([' ', tens_digit(seconds), ones_digit(seconds), ' ']);

    button.wait_for_rising_edge().await;
    if let Either::Second(()) = select(
        Timer::after(Duration::from_secs(1)),
        button.wait_for_falling_edge(),
    )
    .await
    {
        State::ShowMinutes
    } else {
        State::EditSeconds
    }
}

async fn edit_seconds_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &mut AdjustableClock,
) -> State {
    virtual_display.write_chars([' ', '0', '0', ' ']);
    button.wait_for_rising_edge().await;
    let (_, _, seconds) = adjustable_clock.h_m_s();
    let till_next_minute = ONE_MIN - Duration::from_secs(seconds.into());
    *adjustable_clock += till_next_minute;
    State::ShowMinutes
}

async fn show_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &AdjustableClock,
) -> State {
    let (_, minutes, _) = adjustable_clock.h_m_s();

    virtual_display.write_chars([' ', ' ', tens_digit(minutes), ones_digit(minutes)]);

    button.wait_for_rising_edge().await;
    if let Either::Second(()) = select(
        Timer::after(Duration::from_secs(1)),
        button.wait_for_falling_edge(),
    )
    .await
    {
        State::ShowHours
    } else {
        State::EditMinutes
    }
}

async fn edit_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &mut AdjustableClock,
) -> State {
    loop {
        if let Either::Second(()) = select(
            Timer::after(Duration::from_millis(250)),
            button.wait_for_rising_edge(),
        )
        .await
        {
            return State::ShowHours;
        }
        *adjustable_clock += ONE_MIN;
        let (_, minutes, _) = adjustable_clock.h_m_s();

        virtual_display.write_chars([' ', ' ', tens_digit(minutes), ones_digit(minutes)]);
    }
}

async fn show_hours_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &AdjustableClock,
) -> State {
    let (hours, _, _) = adjustable_clock.h_m_s();

    virtual_display.write_chars([
        if hours >= 10 { '1' } else { ' ' },
        ones_digit(hours),
        ' ',
        ' ',
    ]);

    button.wait_for_rising_edge().await;
    if let Either::Second(()) = select(
        Timer::after(Duration::from_secs(1)),
        button.wait_for_falling_edge(),
    )
    .await
    {
        State::DisplayOff
    } else {
        State::EditHours
    }
}

async fn edit_hours_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &mut AdjustableClock,
) -> State {
    loop {
        if let Either::Second(()) = select(
            Timer::after(Duration::from_millis(500)),
            button.wait_for_rising_edge(),
        )
        .await
        {
            return State::DisplayOff;
        }
        *adjustable_clock += ONE_HOUR;
        let (hours, _, _) = adjustable_clock.h_m_s();

        virtual_display.write_chars([
            if hours >= 10 { '1' } else { ' ' },
            ones_digit(hours),
            ' ',
            ' ',
        ]);
    }
}

async fn display_off_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    _adjustable_clock: &AdjustableClock,
) -> State {
    virtual_display.write_chars([' ', ' ', ' ', ' ']);
    button.wait_for_rising_edge().await;
    button.wait_for_falling_edge().await;
    State::Last
}

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
