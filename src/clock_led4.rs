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
        #[cfg(all(feature = "wifi", not(feature = "host")))]
        timezone_field: &'static crate::wifi_auto::fields::TimezoneField,
        spawner: Spawner,
    ) -> crate::Result<Self> {
        let led4 = Led4::new(
            clock_led4_static.led(),
            cell_pins,
            segment_pins,
            spawner,
        )?;
        #[cfg(all(feature = "wifi", not(feature = "host")))]
        let offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
        #[cfg(not(all(feature = "wifi", not(feature = "host"))))]
        let offset_minutes = 0;
        let token = clock_led4_device_loop(
            clock_led4_static.commands(),
            led4,
            offset_minutes,
            clock_led4_static.utc_offset_mirror(),
            #[cfg(all(feature = "wifi", not(feature = "host")))]
            timezone_field,
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

    /// Run the clock state machine loop.
    ///
    /// This method runs indefinitely, executing the state machine and handling
    /// button presses and time sync events. It should be called after WiFi
    /// connection is established and time sync is available.
    pub async fn run(
        &mut self,
        button: &mut crate::button::Button<'_>,
        time_sync: &crate::time_sync::TimeSync,
    ) -> ! {
        let mut clock_state = ClockLed4State::HoursMinutes;
        loop {
            clock_state = clock_state.execute(self, button, time_sync).await;
        }
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
    pub async fn set_offset_minutes(&self, minutes: i32) {
        self.utc_offset_mirror.store(minutes, Ordering::Relaxed);
        self.commands
            .send(ClockLed4Command::SetUtcOffsetMinutes(minutes))
            .await;
    }

    /// Read the most recently applied UTC offset in minutes.
    #[must_use]
    pub fn offset_minutes(&self) -> i32 {
        self.utc_offset_mirror.load(Ordering::Relaxed)
    }

    /// Display the captive portal setup prompt while waiting for credentials.
    pub async fn show_captive_portal_setup(&self) {
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
    #[cfg(all(feature = "wifi", not(feature = "host")))]
    timezone_field: &'static crate::wifi_auto::fields::TimezoneField,
) -> ! {
    let mut clock_time = ClockTime::new(initial_utc_offset_minutes, utc_offset_mirror);
    let mut clock_state = ClockLed4State::default();
    #[cfg(all(feature = "wifi", not(feature = "host")))]
    let mut persisted_offset_minutes = initial_utc_offset_minutes;

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

        // Save timezone offset to flash when it changes.
        #[cfg(all(feature = "wifi", not(feature = "host")))]
        {
            let current_offset_minutes = utc_offset_mirror.load(Ordering::Relaxed);
            if current_offset_minutes != persisted_offset_minutes {
                let _ = timezone_field.set_offset_minutes(current_offset_minutes);
                persisted_offset_minutes = current_offset_minutes;
            }
        }
    }
}
