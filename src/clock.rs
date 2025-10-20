//! Clock virtual device - manages time keeping and emits time tick events

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use core::fmt;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Instant, Timer};
use heapless::String;
use time::{OffsetDateTime, UtcOffset};

use crate::unix_seconds::UnixSeconds;
use crate::{Error, Result};

// ============================================================================
// Types
// ============================================================================

#[derive(Clone, Copy)]
pub enum ClockState {
    NotSet,
    Synced,
}

#[derive(Clone, Copy)]
pub struct ClockEvent {
    pub datetime: OffsetDateTime,
    pub state: ClockState,
}

pub enum ClockCommand {
    SetTime { unix_seconds: UnixSeconds },
}

// ============================================================================
// Clock Virtual Device
// ============================================================================

pub type ClockCommands = Channel<CriticalSectionRawMutex, ClockCommand, 4>;
pub type ClockEvents = Signal<CriticalSectionRawMutex, ClockEvent>;

/// Resources needed by Clock device
pub struct ClockNotifier {
    commands: ClockCommands,
    events: ClockEvents,
}

/// Clock virtual device - manages time keeping and emits time tick events
pub struct Clock {
    commands: &'static ClockCommands,
    events: &'static ClockEvents,
}

impl Clock {
    /// Create Clock resources
    #[must_use]
    pub const fn notifier() -> ClockNotifier {
        ClockNotifier {
            commands: Channel::new(),
            events: Signal::new(),
        }
    }

    /// Create a new Clock device and spawn its task
    pub fn new(notifier: &'static ClockNotifier, spawner: Spawner) -> Self {
        unwrap!(spawner.spawn(clock_device_loop(notifier)));
        Self {
            commands: &notifier.commands,
            events: &notifier.events,
        }
    }

    /// Wait for and return the next clock tick event
    pub async fn wait(&self) -> ClockEvent {
        self.events.wait().await
    }

    /// Send a command to set the time
    pub async fn set_time(&self, unix_seconds: UnixSeconds) {
        self.commands.send(ClockCommand::SetTime { unix_seconds }).await;
    }

    /// Format 24-hour time as 12-hour with AM/PM
    #[must_use]
    fn format_12hour(hours: u8) -> (u8, &'static str) {
        if hours == 0 {
            (12, "AM")
        } else if hours < 12 {
            (hours, "AM")
        } else if hours == 12 {
            (12, "PM")
        } else {
            #[expect(clippy::arithmetic_side_effects, reason = "hour guaranteed 13-23")]
            (hours - 12, "PM")
        }
    }

    /// Format time info as display string
    pub fn format_display(time_info: &ClockEvent) -> Result<String<64>> {
        let mut text = String::<64>::new();
        
        let dt = time_info.datetime;
        let (hour12, am_pm) = Self::format_12hour(dt.hour());
        
        match time_info.state {
            ClockState::NotSet => {
                fmt::Write::write_fmt(
                    &mut text,
                    format_args!(
                        "{:2}:{:02}:{:02} {}\nTime not set",
                        hour12,
                        dt.minute(),
                        dt.second(),
                        am_pm
                    ),
                )
                .map_err(|_| Error::FormatError)?;
            }
            ClockState::Synced => {
                fmt::Write::write_fmt(
                    &mut text,
                    format_args!(
                        "{:2}:{:02}:{:02} {}\n{:04}-{:02}-{:02}",
                        hour12,
                        dt.minute(),
                        dt.second(),
                        am_pm,
                        dt.year(),
                        u8::from(dt.month()),
                        dt.day()
                    ),
                )
                .map_err(|_| Error::FormatError)?;
            }
        }
        
        Ok(text)
    }
}

#[embassy_executor::task]
async fn clock_device_loop(resources: &'static ClockNotifier) -> ! {
    let err = inner_clock_device_loop(resources).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_clock_device_loop(resources: &'static ClockNotifier) -> Result<Infallible> {
    // Read configuration from compile-time environment
    const UTC_OFFSET_MINUTES: &str = env!("UTC_OFFSET_MINUTES");
    let offset_minutes: i32 = UTC_OFFSET_MINUTES.parse().unwrap_or(0);
    
    // Create UtcOffset from minutes
    #[expect(clippy::arithmetic_side_effects, reason = "offset bounds checked")]
    let utc_offset = UtcOffset::from_whole_seconds(offset_minutes * 60)
        .unwrap_or(UtcOffset::UTC);

    info!(
        "Clock device started (UTC offset: {} minutes)",
        offset_minutes
    );

    // Monotonic anchor for drift-free timekeeping
    let mut base_unix_seconds: Option<UnixSeconds> = None;
    let mut base_instant: Option<Instant> = None;
    let mut clock_state = ClockState::NotSet;

    // For initial "Time not set" display, start from midnight
    let mut current_time: OffsetDateTime = OffsetDateTime::from_unix_timestamp(0).expect("midnight is valid");

    loop {
        // Emit tick event
        let time_info = ClockEvent {
            datetime: current_time,
            state: clock_state,
        };
        resources.events.signal(time_info);

        // Wait for either 1 second or a command
        match select(Timer::after_secs(1), resources.commands.receive()).await {
            Either::First(_) => {
                // Timer elapsed - compute time from monotonic anchor
                if let (Some(base_unix_seconds), Some(base_instant)) = (base_unix_seconds, base_instant) {
                    let elapsed = (Instant::now() - base_instant).as_secs();
                    let unix_seconds = UnixSeconds(base_unix_seconds.as_i64().saturating_add(elapsed as i64));
                    current_time = unix_seconds.to_offset_datetime(utc_offset).expect("valid offset datetime");
                } else {
                    // Fallback for "Time not set" - simple increment
                    current_time = current_time.checked_add(time::Duration::seconds(1)).unwrap_or(current_time);
                }
            }
            Either::Second(cmd) => {
                // Command received
                match cmd {
                    ClockCommand::SetTime { unix_seconds } => {
                        // Set monotonic anchor
                        base_unix_seconds = Some(unix_seconds);
                        base_instant = Some(Instant::now());
                        clock_state = ClockState::Synced;
                        
                        // Update current time
                        current_time = unix_seconds.to_offset_datetime(utc_offset).expect("valid offset datetime");
                        
                        info!(
                            "Clock time set: {} (offset={} minutes)",
                            unix_seconds.as_i64(),
                            offset_minutes
                        );

                        // Emit immediate tick with new time
                        let time_info = ClockEvent {
                            datetime: current_time,
                            state: clock_state,
                        };
                        resources.events.signal(time_info);
                    }
                }
            }
        }
    }
}
