use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

use crate::{
    blinker::{BlinkMode, Blinker, BlinkerNotifier},
    offset_time::OffsetTime,
    pins::OutputArray,
    shared_constants::{CELL_COUNT0, ONE_DAY, ONE_HOUR, ONE_MINUTE, ONE_SECOND, SEGMENT_COUNT0},
};

pub struct Clock<'a>(&'a NotifierInner);
type NotifierInner = Channel<CriticalSectionRawMutex, ClockNotice, 4>;
pub type ClockNotifier = (NotifierInner, BlinkerNotifier);

impl Clock<'_> {
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        digit_pins: OutputArray<'static, CELL_COUNT0>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT0>,
        notifier: &'static ClockNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (notifier_inner, blinker_notifier) = notifier;
        let clock = Self(notifier_inner);
        let blinkable_display = Blinker::new(digit_pins, segment_pins, blinker_notifier, spawner)?;
        spawner.spawn(device_loop(blinkable_display, notifier_inner))?;
        Ok(clock)
    }

    #[must_use]
    pub const fn notifier() -> ClockNotifier {
        (Channel::new(), Blinker::notifier())
    }

    pub async fn set_mode(&self, clock_mode: ClockMode) {
        self.0.send(ClockNotice::SetMode { clock_mode }).await;
    }

    pub async fn adjust_offset(&self, delta: Duration) {
        self.0.send(ClockNotice::AdjustOffset(delta)).await;
    }

    pub async fn reset_seconds(&self) {
        self.0.send(ClockNotice::ResetSeconds).await;
    }
}

pub enum ClockNotice {
    SetMode { clock_mode: ClockMode },
    AdjustOffset(Duration),
    ResetSeconds,
}

impl ClockNotice {
    /// Handles the action associated with the given `ClockNotice`.
    pub fn apply(self, offset_time: &mut OffsetTime, clock_mode: &mut ClockMode) {
        match self {
            ClockNotice::AdjustOffset(delta) => {
                *offset_time += delta;
            }
            ClockNotice::SetMode {
                clock_mode: new_clock_mode,
            } => {
                *clock_mode = new_clock_mode;
            }
            ClockNotice::ResetSeconds => {
                let sleep_duration = OffsetTime::till_next(offset_time.now(), ONE_MINUTE);
                *offset_time += sleep_duration;
            }
        }
    }
}

pub enum ClockMode {
    HoursMinutes,
    MinutesSeconds,
    BlinkingSeconds,
    SecondsZero,
    BlinkingMinutes,
    SolidMinutes,
    BlinkingHours,
    SolidHours,
}

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn device_loop(
    blinkable_display: Blinker<'static>,
    clock_notifier: &'static NotifierInner,
) -> ! {
    let mut offset_time = OffsetTime::default();
    let mut clock_mode = ClockMode::MinutesSeconds;

    loop {
        // Compute the display and time until the display change.
        let (chars, blink_mode, sleep_duration) = clock_mode.display_info(&offset_time);
        blinkable_display.write_chars(chars, blink_mode);

        // Wait for a notification or for the sleep duration to elapse
        info!("Sleep for {:?}", sleep_duration);
        if let Either::First(notification) =
            select(clock_notifier.receive(), Timer::after(sleep_duration)).await
        {
            notification.apply(&mut offset_time, &mut clock_mode);
        }
    }
}

impl ClockMode {
    /// Main helper method to compute display characters, blink mode, and sleep duration.
    pub fn display_info(&self, offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        match self {
            ClockMode::HoursMinutes => Self::hours_minutes(offset_time),
            ClockMode::MinutesSeconds => Self::minutes_seconds(offset_time),
            ClockMode::BlinkingSeconds => Self::blinking_seconds(offset_time),
            ClockMode::SecondsZero => Self::seconds_zero(),
            ClockMode::BlinkingMinutes => Self::blinking_minutes(offset_time),
            ClockMode::SolidMinutes => Self::solid_minutes(offset_time),
            ClockMode::BlinkingHours => Self::blinking_hours(offset_time),
            ClockMode::SolidHours => Self::solid_hours(offset_time),
        }
    }

    /// Helper functions for each mode
    fn hours_minutes(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        let (hours, minutes, _, sleep_duration) = offset_time.h_m_s_sleep_duration(ONE_MINUTE);
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

    fn minutes_seconds(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, minutes, seconds, sleep_duration) = offset_time.h_m_s_sleep_duration(ONE_SECOND);
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

    fn blinking_seconds(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, _, seconds, sleep_duration) = offset_time.h_m_s_sleep_duration(ONE_SECOND);
        (
            [' ', tens_digit(seconds), ones_digit(seconds), ' '],
            BlinkMode::BlinkingAndOn,
            sleep_duration,
        )
    }

    fn seconds_zero() -> ([char; 4], BlinkMode, Duration) {
        // We don't really need to wake up even once a day to update
        // the constant "00:00" display, but Duration::MAX causes an overflow
        // and this is simple.
        ([' ', '0', '0', ' '], BlinkMode::Solid, ONE_DAY)
    }

    fn blinking_minutes(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, minutes, _, sleep_duration) = offset_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            [' ', ' ', tens_digit(minutes), ones_digit(minutes)],
            BlinkMode::BlinkingAndOn,
            sleep_duration,
        )
    }

    fn solid_minutes(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        let (_, minutes, _, sleep_duration) = offset_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            [' ', ' ', tens_digit(minutes), ones_digit(minutes)],
            BlinkMode::Solid,
            sleep_duration,
        )
    }

    fn blinking_hours(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        let (hours, _, _, sleep_duration) = offset_time.h_m_s_sleep_duration(ONE_HOUR);
        (
            [tens_hours(hours), ones_digit(hours), ' ', ' '],
            BlinkMode::BlinkingAndOn,
            sleep_duration,
        )
    }

    fn solid_hours(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
        let (hours, _, _, sleep_duration) = offset_time.h_m_s_sleep_duration(ONE_HOUR);
        (
            [tens_hours(hours), ones_digit(hours), ' ', ' '],
            BlinkMode::Solid,
            sleep_duration,
        )
    }
}

#[inline]
fn tens_digit(value: u8) -> char {
    ((value / 10) + b'0') as char
}

#[inline]
fn tens_hours(value: u8) -> char {
    if value >= 10 {
        '1'
    } else {
        ' '
    }
}

#[inline]
fn ones_digit(value: u8) -> char {
    ((value % 10) + b'0') as char
}
