//! A device abstraction for 4-digit LED clocks.

pub mod state;
pub mod time;

use core::sync::atomic::{AtomicI32, Ordering};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

use self::state::ClockLed4State;
use self::time::ClockTime;
use crate::clock_led4::time::ONE_MINUTE;
use crate::led4::OutputArray;
use crate::led4::{CELL_COUNT, SEGMENT_COUNT};
use crate::led4::{Led4, Led4Static};

/// A device abstraction for a 4-digit LED clock.
pub struct ClockLed4<'a> {
    commands: &'a ClockLed4OuterStatic,
    utc_offset_mirror: &'a AtomicI32,
}
/// Static type for the `ClockLed4` device abstraction.
pub struct ClockLed4Static {
    commands: ClockLed4OuterStatic,
    led: Led4Static,
    utc_offset_minutes: AtomicI32,
}
/// Channel type for sending commands to the `ClockLed4` device.
pub type ClockLed4OuterStatic = Channel<CriticalSectionRawMutex, ClockLed4Command, 4>;

impl ClockLed4Static {
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            commands: Channel::new(),
            led: Led4::new_static(),
            utc_offset_minutes: AtomicI32::new(0),
        }
    }

    fn commands(&'static self) -> &'static ClockLed4OuterStatic {
        &self.commands
    }

    fn led(&'static self) -> &'static Led4Static {
        &self.led
    }

    fn utc_offset_mirror(&'static self) -> &'static AtomicI32 {
        &self.utc_offset_minutes
    }
}

impl ClockLed4<'_> {
    /// Create a new `ClockLed4` instance, which entails starting an Embassy task.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        clock_led4_static: &'static ClockLed4Static,
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        initial_utc_offset_minutes: i32,
        spawner: Spawner,
    ) -> crate::Result<Self> {
        let blinkable_display = Led4::new(
            clock_led4_static.led(),
            cell_pins,
            segment_pins,
            spawner,
        )?;
        let token = clock_led4_device_loop(
            clock_led4_static.commands(),
            blinkable_display,
            initial_utc_offset_minutes,
            clock_led4_static.utc_offset_mirror(),
        )?;
        spawner.spawn(token);
        Ok(Self {
            commands: clock_led4_static.commands(),
            utc_offset_mirror: clock_led4_static.utc_offset_mirror(),
        })
    }

    /// Creates a new `ClockLed4Static` instance.
    #[must_use]
    pub const fn new_static() -> ClockLed4Static {
        ClockLed4Static::new_static()
    }

    /// Set the clock state directly.
    pub async fn set_state(&self, clock_state: ClockLed4State) {
        self.commands
            .send(ClockLed4Command::SetState(clock_state))
            .await;
    }

    /// Set the time from Unix seconds.
    pub async fn set_time_from_unix(&self, unix_seconds: crate::unix_seconds::UnixSeconds) {
        self.commands
            .send(ClockLed4Command::SetTimeFromUnix(unix_seconds))
            .await;
    }

    /// Adjust the UTC offset by the given number of hours.
    pub async fn adjust_utc_offset_hours(&self, hours: i32) {
        self.commands
            .send(ClockLed4Command::AdjustUtcOffsetHours(hours))
            .await;
    }

    /// Set the UTC offset in minutes directly.
    pub async fn set_utc_offset_minutes(&self, minutes: i32) {
        self.utc_offset_mirror.store(minutes, Ordering::Relaxed);
        self.commands
            .send(ClockLed4Command::SetUtcOffsetMinutes(minutes))
            .await;
    }

    /// Read the most recently applied UTC offset in minutes.
    #[must_use]
    pub fn utc_offset_minutes(&self) -> i32 {
        self.utc_offset_mirror.load(Ordering::Relaxed)
    }

    /// Display the access point setup prompt while waiting for credentials.
    pub async fn show_access_point_setup(&self) {
        self.commands
            .send(ClockLed4Command::SetState(ClockLed4State::CaptivePortalReady))
            .await;
    }
}

/// Commands sent to the 4-digit LED clock device.
pub enum ClockLed4Command {
    SetState(ClockLed4State),
    SetTimeFromUnix(crate::unix_seconds::UnixSeconds),
    AdjustClockTime(Duration),
    ResetSeconds,
    AdjustUtcOffsetHours(i32),
    SetUtcOffsetMinutes(i32),
}

impl ClockLed4Command {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The += operator wraps to always produce a result less than one day."
    )]
    pub(crate) fn apply(self, clock_time: &mut ClockTime, clock_state: &mut ClockLed4State) {
        match self {
            Self::SetTimeFromUnix(unix_seconds) => {
                clock_time.set_from_unix(unix_seconds);
            }
            Self::AdjustClockTime(delta) => {
                *clock_time += delta;
            }
            Self::SetState(new_clock_mode) => {
                *clock_state = new_clock_mode;
            }
            Self::ResetSeconds => {
                let sleep_duration = ClockTime::till_next(clock_time.now(), ONE_MINUTE);
                *clock_time += sleep_duration;
            }
            Self::AdjustUtcOffsetHours(hours) => {
                clock_time.adjust_utc_offset_hours(hours);
            }
            Self::SetUtcOffsetMinutes(minutes) => {
                clock_time.set_utc_offset_minutes(minutes);
            }
        }
    }
}

#[embassy_executor::task]
async fn clock_led4_device_loop(
    clock_commands: &'static ClockLed4OuterStatic,
    blinker: Led4<'static>,
    initial_utc_offset_minutes: i32,
    utc_offset_mirror: &'static AtomicI32,
) -> ! {
    let mut clock_time = ClockTime::new(initial_utc_offset_minutes, utc_offset_mirror);
    let mut clock_state = ClockLed4State::default();

    loop {
        let (blink_mode, text, sleep_duration) = clock_state.render(&clock_time);
        blinker.write_text(blink_mode, text);

        #[cfg(feature = "display-trace")]
        info!("Sleep for {:?}", sleep_duration);
        if let Either::First(notification) =
            select(clock_commands.receive(), Timer::after(sleep_duration)).await
        {
            notification.apply(&mut clock_time, &mut clock_state);
        }
    }
}
