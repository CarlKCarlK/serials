#![no_std]
#![no_main]

use core::array;

use defmt::unwrap;
use embassy_executor::{Executor, Spawner};
use embassy_futures::select::{select, Either};
use embassy_rp::{
    gpio,
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use embedded_hal::digital::OutputPin;
use gpio::Level;
use static_cell::StaticCell;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
use heapless::{LinearMap, Vec};
use {defmt_rtt as _, panic_probe as _}; // Adjust the import path according to your setup

enum State {
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
        const SECONDS_PER_FIVE_MINUTES: u64 = 5 * 60;
        let seconds_to_five_minutes =
            SECONDS_PER_FIVE_MINUTES - (elapsed_second % SECONDS_PER_FIVE_MINUTES);
        let (hours, minutes) = (
            (elapsed_minutes / 60 % 12) as usize,
            (elapsed_minutes % 60) as usize,
        );
        let mut bytes = [0u8; DIGIT_COUNT1];
        let (digit_index, byte) = TWELVE_TO_OUTSIDE_DIGIT_INDEX_AND_BYTE[hours];
        bytes[digit_index] |= byte;
        let (digit_index, byte) = TWELVE_TO_INSIDE_DIGIT_INDEX_AND_BYTE[minutes / 5];
        bytes[digit_index] |= byte;
        VIRTUAL_DISPLAY1.write_bytes(&bytes).await;

        if let Either::Second(()) = select(
            Timer::after(Duration::from_secs(seconds_to_five_minutes)), // cmk change to type to next tick
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

#[embassy_executor::main]
async fn main(_spawner0: Spawner) {
    let (pins, core1) = Pins::new_and_core1();

    // Spawn 'multiplex_display1' on core1
    spawn_core1(
        core1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner1| {
                unwrap!(spawner1.spawn(monitor_display1(pins.digits1, pins.segments1)));
            });
        },
    );

    let start = Instant::now();
    let mut offset = Duration::default();
    let button = pins.button;

    let mut state = State::First;
    loop {
        state = match state {
            State::First => State::DisplayHoursMinutes,
            State::DisplayHoursMinutes => display_hours_minutes_state(button, start, &offset).await,
            State::DisplayMinutesSeconds => {
                display_minutes_seconds_state(button, start, &offset).await
            }
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
    }
}

struct Pins {
    digits1: &'static mut [gpio::Output<'static>; DIGIT_COUNT1],
    segments1: &'static mut [gpio::Output<'static>; 8],
    button: &'static mut gpio::Input<'static>,
    _led0: &'static mut gpio::Output<'static>,
}

impl Pins {
    fn new_and_core1() -> (Self, CORE1) {
        let p: embassy_rp::Peripherals = embassy_rp::init(Default::default());
        let core1 = p.CORE1;

        static DIGIT_PINS1: StaticCell<[gpio::Output; DIGIT_COUNT1]> = StaticCell::new();
        let digits1 = DIGIT_PINS1.init([
            gpio::Output::new(p.PIN_1, Level::High),
            gpio::Output::new(p.PIN_2, Level::High),
            gpio::Output::new(p.PIN_3, Level::High),
            gpio::Output::new(p.PIN_4, Level::High),
        ]);

        static SEGMENT_PINS1: StaticCell<[gpio::Output; 8]> = StaticCell::new();
        let segments1 = SEGMENT_PINS1.init([
            gpio::Output::new(p.PIN_5, Level::Low),
            gpio::Output::new(p.PIN_6, Level::Low),
            gpio::Output::new(p.PIN_7, Level::Low),
            gpio::Output::new(p.PIN_8, Level::Low),
            gpio::Output::new(p.PIN_9, Level::Low),
            gpio::Output::new(p.PIN_10, Level::Low),
            gpio::Output::new(p.PIN_11, Level::Low),
            gpio::Output::new(p.PIN_12, Level::Low),
        ]);

        static BUTTON_PIN: StaticCell<gpio::Input> = StaticCell::new();
        let button = BUTTON_PIN.init(gpio::Input::new(p.PIN_13, gpio::Pull::Down));

        static LED0_PIN: StaticCell<gpio::Output> = StaticCell::new();
        let led0 = LED0_PIN.init(gpio::Output::new(p.PIN_0, Level::Low));

        (
            Self {
                digits1,
                segments1,
                button,
                _led0: led0,
            },
            core1,
        )
    }
}

// cmk why not have the channel send the bytes directly?
pub struct VirtualDisplay<const DIGIT_COUNT: usize> {
    mutex_digits: Mutex<CriticalSectionRawMutex, [u8; DIGIT_COUNT]>,
    update_display_channel: Channel<CriticalSectionRawMutex, (), 1>,
}

// Display #1 is a 4-digit 7-segment display
pub const DIGIT_COUNT1: usize = 4;

static VIRTUAL_DISPLAY1: VirtualDisplay<DIGIT_COUNT1> = VirtualDisplay {
    mutex_digits: Mutex::new([255; DIGIT_COUNT1]),
    update_display_channel: Channel::new(),
};

#[embassy_executor::task]
async fn monitor_display1(
    digit_pins: &'static mut [gpio::Output<'_>; DIGIT_COUNT1],
    segment_pins: &'static mut [gpio::Output<'_>; 8],
) {
    VIRTUAL_DISPLAY1.monitor(digit_pins, segment_pins).await;
}

// cmk would be nice to have a separate way to turn on decimal points
// cmk would be nice to have a way to pass in 4 chars
impl<const DIGIT_COUNT: usize> VirtualDisplay<DIGIT_COUNT> {
    pub async fn write_text(&'static self, text: &str) {
        let bytes = line_to_u8_array(text);
        self.write_bytes(&bytes).await;
    }
    pub async fn write_bytes(&'static self, bytes_in: &[u8; DIGIT_COUNT]) {
        {
            // inner scope to release the lock
            let mut bytes_out = self.mutex_digits.lock().await;
            for (byte_out, byte_in) in bytes_out.iter_mut().zip(bytes_in.iter()) {
                *byte_out = *byte_in;
            }
        }
        // Say that the display should be updated. If a previous update is
        // still pending, this new update can be ignored.
        let _ = self.update_display_channel.try_send(());
    }

    pub async fn write_number(&'static self, mut number: u16, padding: u8) {
        let mut bytes = [padding; DIGIT_COUNT];

        for i in (0..DIGIT_COUNT).rev() {
            let digit = (number % 10) as usize; // Get the last digit
            bytes[i] = Leds::DIGITS[digit];
            number /= 10; // Remove the last digit
            if number == 0 {
                break;
            }
        }

        // If the original number was out of range, turn on all decimal points
        if number > 0 {
            for byte in bytes.iter_mut() {
                *byte |= Leds::DECIMAL;
            }
        }
        self.write_bytes(&bytes).await;
    }

    #[allow(clippy::needless_range_loop)]
    async fn monitor(
        &'static self,
        digit_pins: &'static mut [gpio::Output<'_>; DIGIT_COUNT],
        segment_pins: &'static mut [gpio::Output<'_>; 8],
    ) {
        loop {
            // How many unique, non-blank digits?
            let mut map: LinearMap<u8, Vec<usize, DIGIT_COUNT>, DIGIT_COUNT> = LinearMap::new();
            {
                // inner scope to release the lock
                let digits = self.mutex_digits.lock().await;
                let digits = digits.iter();
                for (index, byte) in digits.enumerate() {
                    if *byte != 0 {
                        if let Some(vec) = map.get_mut(byte) {
                            vec.push(index).unwrap();
                        } else {
                            let mut vec = Vec::default();
                            vec.push(index).unwrap();
                            map.insert(*byte, vec).unwrap();
                        }
                    }
                }
            }
            match map.len() {
                // If the display should be empty, then just wait for the next update
                0 => self.update_display_channel.receive().await,
                // If only one pattern should be displayed (even on multiple digits), display it
                // and wait for the next update
                1 => {
                    // get one and only key and value
                    let (byte, indexes) = map.iter().next().unwrap();
                    // Set the segment pins with the bool iterator
                    bool_iter(*byte).zip(segment_pins.iter_mut()).for_each(
                        |(state, segment_pin)| {
                            segment_pin.set_state(state.into()).unwrap();
                        },
                    );
                    // activate the digits, wait for the next update, and deactivate the digits
                    for digit_index in indexes.iter() {
                        digit_pins[*digit_index].set_low(); // Assuming common cathode setup
                    }
                    self.update_display_channel.receive().await;
                    for digit_index in indexes.iter() {
                        digit_pins[*digit_index].set_high();
                    }
                }
                // If multiple patterns should be displayed, multiplex them until the next update
                _ => {
                    loop {
                        for (byte, indexes) in map.iter() {
                            // Set the segment pins with the bool iterator
                            bool_iter(*byte).zip(segment_pins.iter_mut()).for_each(
                                |(state, segment_pin)| {
                                    segment_pin.set_state(state.into()).unwrap();
                                },
                            );
                            // Activate, pause, and deactivate the digits
                            for digit_index in indexes.iter() {
                                digit_pins[*digit_index].set_low(); // Assuming common cathode setup
                            }
                            let sleep = 3; // cmk maybe this should depend on the # of digits
                                           // Sleep (but wake up early if the display should be updated)
                            select(
                                Timer::after(Duration::from_millis(sleep)),
                                self.update_display_channel.receive(),
                            )
                            .await;
                            for digit_index in indexes.iter() {
                                digit_pins[*digit_index].set_high();
                            }

                            // // cmk sleep for a bit with all off
                            // let sleep = 3; // cmk 3 is too long
                            // // Sleep (but wake up early if the display should be updated)
                            // select(
                            //     Timer::after(Duration::from_millis(sleep)),
                            //     self.update_display_channel.receive(),
                            // )
                            // .await;
                        }
                        // break out of multiplexing loop if the display should be updated
                        if self.update_display_channel.try_receive().is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Turn a u8 into an iterator of bool
    pub async fn bool_iter(&'static self, digit_index: usize) -> array::IntoIter<bool, 8> {
        // inner scope to release the lock
        let byte: u8;
        {
            let digit_array = self.mutex_digits.lock().await;
            byte = digit_array[digit_index];
        }
        bool_iter(byte)
    }
}

#[inline]
/// Turn a u8 into an iterator of bool
pub fn bool_iter(mut byte: u8) -> array::IntoIter<bool, 8> {
    // turn a u8 into an iterator of bool
    let mut bools_out = [false; 8];
    for bool_out in bools_out.iter_mut() {
        *bool_out = byte & 1 == 1;
        byte >>= 1;
    }
    bools_out.into_iter()
}

fn line_to_u8_array<const DIGIT_COUNT: usize>(line: &str) -> [u8; DIGIT_COUNT] {
    let mut result = [0; DIGIT_COUNT];
    (0..DIGIT_COUNT).zip(line.chars()).for_each(|(i, c)| {
        result[i] = Leds::ASCII_TABLE[c as usize];
    });
    if line.len() > DIGIT_COUNT {
        for byte in result.iter_mut() {
            *byte |= Leds::DECIMAL;
        }
    }
    result
}

pub struct Leds;

#[allow(dead_code)]
impl Leds {
    const SEG_A: u8 = 0b00000001;
    const SEG_B: u8 = 0b00000010;
    const SEG_C: u8 = 0b00000100;
    const SEG_D: u8 = 0b00001000;
    const SEG_E: u8 = 0b00010000;
    const SEG_F: u8 = 0b00100000;
    const SEG_G: u8 = 0b01000000;
    const DECIMAL: u8 = 0b10000000;

    const DIGITS: [u8; 10] = [
        0b00111111, // Digit 0
        0b00000110, // Digit 1
        0b01011011, // Digit 2
        0b01001111, // Digit 3
        0b01100110, // Digit 4
        0b01101101, // Digit 5
        0b01111101, // Digit 6
        0b00000111, // Digit 7
        0b01111111, // Digit 8
        0b01101111, // Digit 9
    ];
    const SPACE: u8 = 0b00000000;

    const ASCII_TABLE: [u8; 128] = [
        // Control characters (0-31) + space (32)
        0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, // 0-4
        0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, // 5-9
        0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, // 10-14
        0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, // 15-19
        0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, //  20-24
        0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, //  25-29
        0b00000000, 0b00000000, 0b00000000, // 30-32
        // Symbols (33-47)
        0b10000110, // !
        0b00000000, // "
        0b00000000, // #
        0b00000000, // $
        0b00000000, // %
        0b00000000, // &
        0b00000000, // '
        0b00000000, // (
        0b00000000, // )
        0b00000000, // *
        0b00000000, // +
        0b00000000, // ,
        0b01000000, // -
        0b10000000, // .
        0b00000000, // /
        // Numbers (48-57)
        0b00111111, // 0
        0b00000110, // 1
        0b01011011, // 2
        0b01001111, // 3
        0b01100110, // 4
        0b01101101, // 5
        0b01111101, // 6
        0b00000111, // 7
        0b01111111, // 8
        0b01101111, // 9
        // Symbols (58-64)
        0b00000000, // :
        0b00000000, // ;
        0b00000000, // <
        0b00000000, // =
        0b00000000, // >
        0b00000000, // ?
        0b00000000, // @
        // Uppercase letters (65-90)
        0b01110111, // A
        0b01111100, // B (same as b)
        0b00111001, // C
        0b01011110, // D (same as d)
        0b01111001, // E
        0b01110001, // F
        0b00111101, // G (same as 9)
        0b01110110, // H
        0b00000110, // I (same as 1)
        0b00011110, // J
        0b01110110, // K (approximation)
        0b00111000, // L
        0b00010101, // M (arbitrary, no good match)
        0b01010100, // N
        0b00111111, // O (same as 0)
        0b01110011, // P
        0b01100111, // Q
        0b01010000, // R
        0b01101101, // S (same as 5)
        0b01111000, // T
        0b00111110, // U
        0b00101010, // V (arbitrary, no good match)
        0b00011101, // W (arbitrary, no good match)
        0b01110110, // X (same as H)
        0b01101110, // Y
        0b01011011, // Z (same as 2)
        // Symbols (91-96)
        0b00111001, // [
        0b00000000, // \
        0b00001111, // ]
        0b00000000, // ^
        0b00001000, // _
        0b00000000, // `
        // Lowercase letters (97-122), reusing uppercase for simplicity
        0b01110111, // A
        0b01111100, // B (same as b)
        0b00111001, // C
        0b01011110, // D (same as d)
        0b01111001, // E
        0b01110001, // F
        0b00111101, // G (same as 9)
        0b01110100, // H
        0b00000110, // I (same as 1)
        0b00011110, // J
        0b01110110, // K (approximation)
        0b00111000, // L
        0b00010101, // M (arbitrary, no good match)
        0b01010100, // N
        0b00111111, // O (same as 0)
        0b01110011, // P
        0b01100111, // Q
        0b01010000, // R
        0b01101101, // S (same as 5)
        0b01111000, // T
        0b00111110, // U
        0b00101010, // V (arbitrary, no good match)
        0b00011101, // W (arbitrary, no good match)
        0b01110110, // X (same as H)
        0b01101110, // Y
        0b01011011, // Z (same as 2)
        // Placeholder for simplicity
        0b00111001, // '{' (123)
        0b00000110, // '|' (124)
        0b00001111, // '}' (125)
        0b01000000, // '~' (126)
        0b00000000, // delete (127)
    ];
}
