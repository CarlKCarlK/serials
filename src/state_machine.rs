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
    state // cmk any way to avoid returning offset?
}

fn display_time(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    adjustable_clock: &AdjustableClock,
) {
    // Time since start in minutes
    let elapsed_minutes = adjustable_clock.now().as_secs() / 60;

    // Calculate the number to display
    #[allow(clippy::cast_possible_truncation)]
    let (hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
    let hours = (hours + 11) % 12 + 1; // 1-12 instead of 0-11
    let number = hours * 100 + minutes;

    virtual_display.write_number(number, 0);
}

const ONE_MIN: Duration = Duration::from_secs(60);
const ONE_HOUR: Duration = Duration::from_secs(60 * 60);

async fn display_hours_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &AdjustableClock,
) -> State {
    loop {
        display_time(virtual_display, adjustable_clock);

        // Sleep until the top of the next minute or until the button is pressed
        let seconds: u64 = adjustable_clock.now().as_secs() % 60;
        let till_next_minute = ONE_MIN - Duration::from_secs(seconds);
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

async fn display_minutes_seconds_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &AdjustableClock,
) -> State {
    loop {
        let now = adjustable_clock.now();
        let elapsed_minutes = now.as_secs() / 60;
        let seconds: u64 = now.as_secs() % 60;
        #[allow(clippy::cast_possible_truncation)]
        let (_hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
        #[allow(clippy::cast_possible_truncation)]
        let d1 = (minutes / 10) as u8 + b'0';
        let d2 = (minutes % 10) as u8 + b'0';
        let d3 = (seconds / 10) as u8 + b'0';
        let d4 = (seconds % 10) as u8 + b'0';
        let text = [d1, d2, d3, d4];
        let text_str: &str = core::str::from_utf8(&text).unwrap();
        virtual_display.write_text(text_str);

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
    let seconds: u64 = adjustable_clock.now().as_secs() % 60;
    let d1 = (seconds / 10) as u8 + b'0';
    let d2 = (seconds % 10) as u8 + b'0';
    let text = [b' ', d1, d2, b' '];
    let text_str: &str = core::str::from_utf8(&text).unwrap();
    virtual_display.write_text(text_str);
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
    virtual_display.write_text(" 00 ");
    button.wait_for_rising_edge().await;
    let seconds: u64 = adjustable_clock.now().as_secs() % 60;
    *adjustable_clock += ONE_MIN - Duration::from_secs(seconds);
    State::ShowMinutes
}

#[allow(clippy::cast_possible_truncation)]
async fn show_minutes_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &AdjustableClock,
) -> State {
    let elapsed_minutes = adjustable_clock.now().as_secs() / 60;

    let (_hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
    let d1 = (minutes / 10) as u8 + b'0';
    let d2 = (minutes % 10) as u8 + b'0';
    let text = [b' ', b' ', d1, d2];
    let text_str: &str = core::str::from_utf8(&text).unwrap();
    virtual_display.write_text(text_str);
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

#[allow(clippy::cast_possible_truncation)]
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
        let elapsed_minutes = adjustable_clock.now().as_secs() / 60;
        let (_hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
        let d1 = (minutes / 10) as u8 + b'0';
        let d2 = (minutes % 10) as u8 + b'0';
        let text = [b' ', b' ', d1, d2];
        let text_str: &str = core::str::from_utf8(&text).unwrap();
        virtual_display.write_text(text_str);
    }
}

#[allow(clippy::cast_possible_truncation)]
async fn show_hours_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    adjustable_clock: &AdjustableClock,
) -> State {
    let elapsed_minutes = adjustable_clock.now().as_secs() / 60;
    let (hours, _minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
    let hours = (hours + 11) % 12 + 1; // 1-12 instead of 0-11
    let d1 = if hours >= 10 { b'1' } else { b' ' };
    let d2 = (hours % 10) as u8 + b'0';
    let text = [d1, d2, b' ', b' '];
    let text_str: &str = core::str::from_utf8(&text).unwrap();
    virtual_display.write_text(text_str);
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
        let elapsed_minutes = adjustable_clock.now().as_secs() / 60;
        #[allow(clippy::cast_possible_truncation)]
        let (hours, _minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
        let hours = (hours + 11) % 12 + 1; // 1-12 instead of 0-11
        let d1 = if hours >= 10 { b'1' } else { b' ' };
        let d2 = (hours % 10) as u8 + b'0';
        let text = [d1, d2, b' ', b' '];
        let text_str: &str = core::str::from_utf8(&text).unwrap();
        virtual_display.write_text(text_str);
    }
}

async fn display_off_state(
    virtual_display: &mut VirtualDisplay<CELL_COUNT0>,
    button: &mut gpio::Input<'_>,
    _adjustable_clock: &AdjustableClock,
) -> State {
    virtual_display.write_text("    ");
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
    pub fn now(&self) -> Duration {
        Instant::now() + self.offset - self.start
    }
}

impl AddAssign<Duration> for AdjustableClock {
    fn add_assign(&mut self, duration: Duration) {
        self.offset += duration;
    }
}
