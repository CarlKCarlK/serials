use embassy_futures::select::{select, Either};
use embassy_rp::gpio;
use embassy_time::{Duration, Instant, Timer};

use crate::{Leds, DIGIT_COUNT1, VIRTUAL_DISPLAY1};

#[derive(Debug, defmt::Format)]
pub(crate) enum State {
    First,
    DisplayHoursMinutes,
    DisplayMinutesSeconds,
    DisplayAnalogHM,
    DisplayAnalogMS,
    ShowSeconds,
    EditSeconds,
    ShowMinutes,
    EditMinutes,
    ShowHours,
    EditHours,
    DisplayOff,
    PowerHog8888,
    PowerHog1204,
    Last,
}

pub(crate) async fn state_to_state(
    mut state: State,
    button: &mut gpio::Input<'_>,
    start: Instant,
    mut offset: Duration,
) -> (State, Duration) {
    state = match state {
        State::First => State::DisplayHoursMinutes,
        State::DisplayHoursMinutes => display_hours_minutes_state(button, start, &offset).await,
        State::DisplayMinutesSeconds => display_minutes_seconds_state(button, start, &offset).await,
        State::DisplayAnalogHM => display_analog_hm_state(button, start, &offset).await,
        State::DisplayAnalogMS => display_analog_ms_state(button, start, &offset).await,
        State::ShowSeconds => show_seconds_state(button, start, &mut offset).await,
        State::EditSeconds => edit_seconds_state(button, start, &mut offset).await,
        State::ShowMinutes => show_minutes_state(button, start, &mut offset).await,
        State::EditMinutes => edit_minutes_state(button, start, &mut offset).await,
        State::ShowHours => show_hours_state(button, start, &mut offset).await,
        State::EditHours => edit_hours_state(button, start, &mut offset).await,
        State::DisplayOff => display_off_state(button, start, &offset).await,
        State::PowerHog8888 => power_hog_state_8888(button, start, &offset).await,
        State::PowerHog1204 => power_hog_state_1204(button, start, &offset).await,
        State::Last => State::First,
    };
    (state, offset) // cmk any way to avoid returning offset?
}

async fn display_time(start: Instant, offset: &Duration) {
    // Time since start in minutes
    let elapsed_minutes = (Instant::now() + *offset - start).as_secs() / 60;

    // Calculate the number to display
    let (hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
    let hours = (hours + 11) % 12 + 1; // 1-12 instead of 0-11
    let number = hours * 100 + minutes;

    VIRTUAL_DISPLAY1.write_number(number, 0).await;
}

const ONE_MIN: Duration = Duration::from_secs(60);
const ONE_HOUR: Duration = Duration::from_secs(60 * 60);

async fn display_hours_minutes_state(
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &Duration,
) -> State {
    loop {
        display_time(start, offset).await;

        // Sleep until the top of the next minute or until the button is pressed
        let seconds: u64 = (Instant::now() + *offset - start).as_secs() % 60;
        let till_next_minute = ONE_MIN - Duration::from_secs(seconds);
        if let Either::Second(()) = select(
            Timer::after(till_next_minute),
            button.wait_for_rising_edge(),
        )
        .await
        {
            button.wait_for_falling_edge().await;
            return State::DisplayMinutesSeconds;
        }
    }
}

async fn display_minutes_seconds_state(
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &Duration,
) -> State {
    loop {
        let now = Instant::now();
        let elapsed_minutes = (now + *offset - start).as_secs() / 60;
        let seconds: u64 = (now + *offset - start).as_secs() % 60;
        let (_hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
        let d1 = (minutes / 10) as u8 + b'0';
        let d2 = (minutes % 10) as u8 + b'0';
        let d3 = (seconds / 10) as u8 + b'0';
        let d4 = (seconds % 10) as u8 + b'0';
        let text = [d1, d2, d3, d4];
        let text_str: &str = core::str::from_utf8(&text).unwrap();
        VIRTUAL_DISPLAY1.write_text(text_str).await;

        if let Either::Second(()) = select(
            Timer::after(Duration::from_secs(1)),
            button.wait_for_rising_edge(),
        )
        .await
        {
            button.wait_for_falling_edge().await;
            return State::DisplayAnalogHM;
        }
    }
}

const TWELVE_TO_OUTSIDE_DIGIT_INDEX_AND_BYTE: [(usize, u8); 12] = [
    (2, Leds::SEG_A), // 12
    (3, Leds::SEG_A), // 1
    (3, Leds::SEG_B), // 2
    (3, Leds::SEG_C), // 3
    (3, Leds::SEG_D), // 4
    (2, Leds::SEG_D), // 5
    (1, Leds::SEG_D), // 6
    (0, Leds::SEG_D), // 7
    (0, Leds::SEG_E), // 8
    (0, Leds::SEG_F), // 9
    (0, Leds::SEG_A), // 10
    (1, Leds::SEG_A), // 11
];

const TWELVE_TO_INSIDE_DIGIT_INDEX_AND_BYTE: [(usize, u8); 12] = [
    (2, Leds::SEG_F), // 0
    (2, Leds::SEG_B), // 5
    (3, Leds::SEG_F), // 10
    (3, Leds::SEG_E), // 15
    (2, Leds::SEG_C), // 20
    (2, Leds::SEG_E), // 25
    (1, Leds::SEG_C), // 30
    (1, Leds::SEG_E), // 35
    (0, Leds::SEG_C), // 40
    (0, Leds::SEG_B), // 45
    (1, Leds::SEG_F), // 50
    (1, Leds::SEG_B), // 55
];

const TWELVE_TO_DASH_DIGIT_INDEX_AND_BYTE: [(usize, u8); 12] = [
    (2, Leds::SEG_G), // 0
    (3, Leds::SEG_G), // 5
    (3, Leds::SEG_G), // 10
    (3, Leds::SEG_G), // 15
    (3, Leds::SEG_G), // 20
    (2, Leds::SEG_G), // 25
    (1, Leds::SEG_G), // 30
    (0, Leds::SEG_G), // 35
    (0, Leds::SEG_G), // 40
    (0, Leds::SEG_G), // 45
    (0, Leds::SEG_G), // 50
    (1, Leds::SEG_G), // 55
];

async fn display_analog_hm_state(
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &Duration,
) -> State {
    loop {
        let now = Instant::now();
        let elapsed_second = (now + *offset - start).as_secs();
        let elapsed_minutes = elapsed_second / 60;
        // const SECONDS_PER_FIVE_MINUTES: u64 = 5 * 60;
        // let seconds_to_five_minutes =
        //     SECONDS_PER_FIVE_MINUTES - (elapsed_second % SECONDS_PER_FIVE_MINUTES);
        const SECONDS_PER_FIVE_SECONDS: u64 = 5;
        let seconds_to_five_seconds =
            SECONDS_PER_FIVE_SECONDS - (elapsed_second % SECONDS_PER_FIVE_SECONDS);

        let (hours, minutes, seconds) = (
            (elapsed_minutes / 60 % 12) as usize,
            (elapsed_minutes % 60) as usize,
            (elapsed_second % 60) as usize,
        );
        let mut bytes = [0u8; DIGIT_COUNT1];
        let (digit_index, byte) = TWELVE_TO_OUTSIDE_DIGIT_INDEX_AND_BYTE[hours];
        bytes[digit_index] |= byte;
        let (digit_index, byte) = TWELVE_TO_INSIDE_DIGIT_INDEX_AND_BYTE[minutes / 5];
        bytes[digit_index] |= byte;
        let (digit_index, byte) = TWELVE_TO_DASH_DIGIT_INDEX_AND_BYTE[seconds / 5];
        bytes[digit_index] |= byte;
        VIRTUAL_DISPLAY1.write_bytes(&bytes).await;

        if let Either::Second(()) = select(
            Timer::after(Duration::from_secs(seconds_to_five_seconds)), // cmk change to type to next tick
            button.wait_for_rising_edge(),
        )
        .await
        {
            button.wait_for_falling_edge().await;
            return State::DisplayAnalogMS;
        }
    }
}

async fn display_analog_ms_state(
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &Duration,
) -> State {
    loop {
        let now = Instant::now();
        let elapsed_second = (now + *offset - start).as_secs();
        let elapsed_minutes = elapsed_second / 60;
        const SECONDS_PER_FIVE_SECONDS: u64 = 5;
        let seconds_to_five_seconds =
            SECONDS_PER_FIVE_SECONDS - (elapsed_second % SECONDS_PER_FIVE_SECONDS);
        let (minutes, seconds) = (
            (elapsed_minutes % 60) as usize,
            (elapsed_second % 60) as usize,
        );
        let mut bytes = [0u8; DIGIT_COUNT1];
        let (digit_index, byte) = TWELVE_TO_OUTSIDE_DIGIT_INDEX_AND_BYTE[minutes / 5];
        bytes[digit_index] |= byte;
        let (digit_index, byte) = TWELVE_TO_INSIDE_DIGIT_INDEX_AND_BYTE[seconds / 5];
        bytes[digit_index] |= byte;
        VIRTUAL_DISPLAY1.write_bytes(&bytes).await;

        if let Either::Second(()) = select(
            Timer::after(Duration::from_secs(seconds_to_five_seconds)), // cmk change to type to next tick
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
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &mut Duration,
) -> State {
    let seconds: u64 = (Instant::now() + *offset - start).as_secs() % 60;
    let d1 = (seconds / 10) as u8 + b'0';
    let d2 = (seconds % 10) as u8 + b'0';
    let text = [b' ', d1, d2, b' '];
    let text_str: &str = core::str::from_utf8(&text).unwrap();
    VIRTUAL_DISPLAY1.write_text(text_str).await;
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
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &mut Duration,
) -> State {
    VIRTUAL_DISPLAY1.write_text(" 00 ").await;
    button.wait_for_rising_edge().await;
    let seconds: u64 = (Instant::now() + *offset - start).as_secs() % 60;
    *offset += ONE_MIN - Duration::from_secs(seconds);
    State::ShowMinutes
}

async fn show_minutes_state(
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &mut Duration,
) -> State {
    let elapsed_minutes = (Instant::now() + *offset - start).as_secs() / 60;
    let (_hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
    let d1 = (minutes / 10) as u8 + b'0';
    let d2 = (minutes % 10) as u8 + b'0';
    let text = [b' ', b' ', d1, d2];
    let text_str: &str = core::str::from_utf8(&text).unwrap();
    VIRTUAL_DISPLAY1.write_text(text_str).await;
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
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &mut Duration,
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
        *offset += ONE_MIN;
        let elapsed_minutes = (Instant::now() + *offset - start).as_secs() / 60;
        let (_hours, minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
        let d1 = (minutes / 10) as u8 + b'0';
        let d2 = (minutes % 10) as u8 + b'0';
        let text = [b' ', b' ', d1, d2];
        let text_str: &str = core::str::from_utf8(&text).unwrap();
        VIRTUAL_DISPLAY1.write_text(text_str).await;
    }
}

async fn show_hours_state(
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &mut Duration,
) -> State {
    let elapsed_minutes = (Instant::now() + *offset - start).as_secs() / 60;
    let (hours, _minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
    let hours = (hours + 11) % 12 + 1; // 1-12 instead of 0-11
    let d1 = if hours >= 10 { b'1' } else { b' ' };
    let d2 = (hours % 10) as u8 + b'0';
    let text = [d1, d2, b' ', b' '];
    let text_str: &str = core::str::from_utf8(&text).unwrap();
    VIRTUAL_DISPLAY1.write_text(text_str).await;
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
    button: &mut gpio::Input<'_>,
    start: Instant,
    offset: &mut Duration,
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
        *offset += ONE_HOUR;
        let elapsed_minutes = (Instant::now() + *offset - start).as_secs() / 60;
        let (hours, _minutes) = ((elapsed_minutes / 60) as u16, (elapsed_minutes % 60) as u16);
        let hours = (hours + 11) % 12 + 1; // 1-12 instead of 0-11
        let d1 = if hours >= 10 { b'1' } else { b' ' };
        let d2 = (hours % 10) as u8 + b'0';
        let text = [d1, d2, b' ', b' '];
        let text_str: &str = core::str::from_utf8(&text).unwrap();
        VIRTUAL_DISPLAY1.write_text(text_str).await;
    }
}

async fn display_off_state(
    button: &mut gpio::Input<'_>,
    _start: Instant,
    _offset: &Duration,
) -> State {
    VIRTUAL_DISPLAY1.write_text("    ").await;
    button.wait_for_rising_edge().await;
    button.wait_for_falling_edge().await;
    State::PowerHog8888
}

async fn power_hog_state_8888(
    button: &mut gpio::Input<'_>,
    _start: Instant,
    _offset: &Duration,
) -> State {
    VIRTUAL_DISPLAY1.write_text("8888").await;
    while button.is_low() {}
    while button.is_high() {}
    State::PowerHog1204
}

async fn power_hog_state_1204(
    button: &mut gpio::Input<'_>,
    _start: Instant,
    _offset: &Duration,
) -> State {
    VIRTUAL_DISPLAY1.write_text("1204").await;
    while button.is_low() {}
    while button.is_high() {}
    State::Last
}
