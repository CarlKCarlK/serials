use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

use crate::{
    blinker::{BlinkMode, Blinker, BlinkerNotifier},
    offset_time::OffsetTime,
    output_array::OutputArray,
    shared_constants::{CELL_COUNT, ONE_DAY, ONE_HOUR, ONE_MINUTE, ONE_SECOND, SEGMENT_COUNT},
};

/// A struct representing a virtual clock.
pub struct Clock<'a>(&'a NotifierInner);
/// Type alias for notifier that sends messages to the `Clock` and the `Blinker` it controls.
pub type ClockNotifier = (NotifierInner, BlinkerNotifier);
/// A type alias for the inner notifier that sends messages to the `Clock`.
///
/// The number is the maximum number of messages that can be stored in the channel without blocking.
pub type NotifierInner = Channel<CriticalSectionRawMutex, ClockNotice, 4>;

impl Clock<'_> {
    /// Creates a new `Clock` instance. cmk
    ///
    /// # Errors
    ///
    /// Returns a `SpawnError` if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static ClockNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let (notifier_inner, blinker_notifier) = notifier;
        let clock = Self(notifier_inner);
        let blinkable_display = Blinker::new(cell_pins, segment_pins, blinker_notifier, spawner)?;
        spawner.spawn(device_loop(blinkable_display, notifier_inner))?;
        Ok(clock)
    }

    #[must_use]
    /// Creates a new `ClockNotifier` instance.
    pub const fn notifier() -> ClockNotifier {
        (Channel::new(), Blinker::notifier())
    }

    pub(crate) async fn set_mode(&self, clock_mode: ClockMode) {
        self.0.send(ClockNotice::SetMode { clock_mode }).await;
    }

    pub(crate) async fn adjust_offset(&self, delta: Duration) {
        self.0.send(ClockNotice::AdjustOffset(delta)).await;
    }

    pub(crate) async fn reset_seconds(&self) {
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
    pub(crate) fn apply(self, offset_time: &mut OffsetTime, clock_mode: &mut ClockMode) {
        match self {
            Self::AdjustOffset(delta) => {
                *offset_time += delta;
            }
            Self::SetMode {
                clock_mode: new_clock_mode,
            } => {
                *clock_mode = new_clock_mode;
            }
            Self::ResetSeconds => {
                let sleep_duration = OffsetTime::till_next(offset_time.now(), ONE_MINUTE);
                *offset_time += sleep_duration;
            }
        }
    }
}

/// Represents the different modes the clock can operate in.
///
/// For example, `HoursMinutes` displays the hours and minutes and `BlinkingSeconds` blinks the seconds
/// to show that they are ready to be reset.
#[allow(missing_docs)] // We don't need to document the variants of this enum.
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
    /// Given a `ClockMode`, compute the characters to display, the blink mode, and the sleep duration.
    pub(crate) fn display_info(
        &self,
        offset_time: &OffsetTime,
    ) -> ([char; 4], BlinkMode, Duration) {
        match self {
            Self::HoursMinutes => Self::hours_minutes(offset_time),
            Self::MinutesSeconds => Self::minutes_seconds(offset_time),
            Self::BlinkingSeconds => Self::blinking_seconds(offset_time),
            Self::SecondsZero => Self::seconds_zero(),
            Self::BlinkingMinutes => Self::blinking_minutes(offset_time),
            Self::SolidMinutes => Self::solid_minutes(offset_time),
            Self::BlinkingHours => Self::blinking_hours(offset_time),
            Self::SolidHours => Self::solid_hours(offset_time),
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

    const fn seconds_zero() -> ([char; 4], BlinkMode, Duration) {
        // We don't really need to wake up even once a day to update
        // the constant "00" display, but Duration::MAX causes an overflow
        // so ONE_DAY is used instead.
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
const fn tens_digit(value: u8) -> char {
    ((value / 10) + b'0') as char
}

#[inline]
const fn tens_hours(value: u8) -> char {
    if value >= 10 {
        '1'
    } else {
        ' '
    }
}

#[inline]
const fn ones_digit(value: u8) -> char {
    ((value % 10) + b'0') as char
}
