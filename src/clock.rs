//! A device abstraction that manages timekeeping and emits tick events.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use time::{OffsetDateTime, UtcOffset};

use crate::Result;
use crate::unix_seconds::UnixSeconds;

// ============================================================================
// Constants
// ============================================================================

/// Duration representing one second.
pub const ONE_SECOND: Duration = Duration::from_secs(1);
/// Duration representing one minute (60 seconds).
pub const ONE_MINUTE: Duration = Duration::from_secs(60);

// ============================================================================
// Types
// ============================================================================

// ClockEvent removed; now clock emits OffsetDateTime directly

/// Extract hour, minute, second from OffsetDateTime
pub fn h_m_s(dt: &OffsetDateTime) -> (u8, u8, u8) {
    let hour = dt.hour() as u8;
    let minute = dt.minute() as u8;
    let second = dt.second() as u8;
    (hour, minute, second)
}

/// Commands sent to the clock device.
enum ClockCommand {
    /// Set the current time from Unix timestamp.
    SetTime { unix_seconds: UnixSeconds },
    /// Update the UTC offset (in minutes).
    SetOffset { minutes: i32 },
    /// Set the tick interval (e.g., ONE_SECOND, ONE_MINUTE).
    SetTickInterval { interval: Duration },
}

// ============================================================================
// Clock Virtual Device
// ============================================================================

/// Channel type for clock commands.
type ClockCommands = Channel<CriticalSectionRawMutex, ClockCommand, 4>;
/// Signal type for clock tick notifications.
type ClockTicks = Signal<CriticalSectionRawMutex, ()>;
/// Current time storage.
type CurrentTime = Mutex<CriticalSectionRawMutex, core::cell::Cell<OffsetDateTime>>;

/// Resources needed by Clock device
pub struct ClockStatic {
    commands: ClockCommands,
    ticks: ClockTicks,
    current_time: CurrentTime,
    offset_minutes: Signal<CriticalSectionRawMutex, i32>,
    tick_interval: Signal<CriticalSectionRawMutex, Duration>,
}

/// A device abstraction that manages time keeping and emits time tick events.
pub struct Clock {
    commands: &'static ClockCommands,
    ticks: &'static ClockTicks,
    current_time: &'static CurrentTime,
    offset_minutes: &'static Signal<CriticalSectionRawMutex, i32>,
}

impl Clock {
    /// Create Clock resources
    #[must_use]
    pub const fn new_static() -> ClockStatic {
        ClockStatic {
            commands: Channel::new(),
            ticks: Signal::new(),
            current_time: Mutex::new(core::cell::Cell::new(OffsetDateTime::UNIX_EPOCH)),
            offset_minutes: Signal::new(),
            tick_interval: Signal::new(),
        }
    }

    /// Create a new Clock device and spawn its task
    pub fn new(clock_static: &'static ClockStatic, offset_minutes: i32, spawner: Spawner) -> Self {
        let tick_interval = ONE_SECOND;
        clock_static.offset_minutes.signal(offset_minutes);
        clock_static.tick_interval.signal(tick_interval);
        let token = unwrap!(clock_device_loop(clock_static));
        spawner.spawn(token);
        Self {
            commands: &clock_static.commands,
            ticks: &clock_static.ticks,
            current_time: &clock_static.current_time,
            offset_minutes: &clock_static.offset_minutes,
        }
    }

    /// Wait for and return the next clock tick event
    pub async fn wait(&self) -> OffsetDateTime {
        self.ticks.wait().await;
        self.current_time.lock(|cell| cell.get())
    }

    /// Get the current time without waiting for a tick.
    /// Returns the most recently computed time.
    pub fn current_time(&self) -> OffsetDateTime {
        self.current_time.lock(|cell| cell.get())
    }

    /// Send a command to set the time
    pub async fn set_time(&self, unix_seconds: UnixSeconds) {
        self.commands
            .send(ClockCommand::SetTime { unix_seconds })
            .await;
    }

    /// Update the UTC offset used for subsequent ticks.
    pub async fn set_offset_minutes(&self, minutes: i32) {
        self.commands
            .send(ClockCommand::SetOffset { minutes })
            .await;
    }

    /// Get the current UTC offset in minutes.
    pub async fn offset_minutes(&self) -> i32 {
        self.offset_minutes.wait().await
    }

    /// Set the tick interval (e.g., Duration::from_secs(1), Duration::from_secs(60)).
    /// The clock will emit events aligned to boundaries (top of second, top of minute, etc.).
    pub async fn set_tick_interval(&self, interval: Duration) {
        self.commands
            .send(ClockCommand::SetTickInterval { interval })
            .await;
    }
}

#[embassy_executor::task]
async fn clock_device_loop(resources: &'static ClockStatic) -> ! {
    let err = inner_clock_device_loop(resources).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_clock_device_loop(resources: &'static ClockStatic) -> Result<Infallible> {
    let mut offset_minutes: i32 = resources.offset_minutes.wait().await;
    #[expect(clippy::arithmetic_side_effects, reason = "offset bounds checked")]
    let mut offset = UtcOffset::from_whole_seconds(offset_minutes * 60).unwrap_or(UtcOffset::UTC);
    let mut tick_interval = resources.tick_interval.wait().await;

    info!(
        "Clock device started (UTC offset: {} minutes, tick interval: {} ms)",
        offset_minutes,
        tick_interval.as_millis()
    );

    // Monotonic anchor for drift-free timekeeping
    let mut base_unix_seconds: Option<UnixSeconds> = None;
    let mut base_instant: Option<Instant> = None;

    // For initial "Time not set" display, start from midnight
    let mut current_time: OffsetDateTime =
        OffsetDateTime::from_unix_timestamp(0).expect("midnight is valid");

    // Helper to calculate duration until next tick boundary
    let sleep_until_boundary = |now_instant: Instant, interval: Duration| -> Duration {
        let elapsed_ticks = now_instant.as_ticks();
        let interval_ticks = interval.as_ticks();
        let ticks_until_next = interval_ticks - (elapsed_ticks % interval_ticks);
        Duration::from_ticks(ticks_until_next)
    };

    loop {
        // Store current time and emit tick notification
        resources.current_time.lock(|cell| cell.set(current_time));
        resources.ticks.signal(());

        // Calculate sleep duration aligned to tick boundary
        let sleep_duration = sleep_until_boundary(Instant::now(), tick_interval);

        // Wait for either tick interval or a command
        match select(Timer::after(sleep_duration), resources.commands.receive()).await {
            Either::First(_) => {
                // Timer elapsed - compute time from monotonic anchor
                if let (Some(base_unix_seconds), Some(base_instant)) =
                    (base_unix_seconds, base_instant)
                {
                    let elapsed = (Instant::now() - base_instant).as_secs();
                    let unix_seconds =
                        UnixSeconds(base_unix_seconds.as_i64().saturating_add(elapsed as i64));
                    current_time = unix_seconds
                        .to_offset_datetime(offset)
                        .expect("valid offset datetime");
                } else {
                    // Fallback for "Time not set" - simple increment
                    current_time = current_time
                        .checked_add(time::Duration::seconds(1))
                        .unwrap_or(current_time);
                }
            }
            Either::Second(cmd) => {
                // Command received
                match cmd {
                    ClockCommand::SetTime { unix_seconds } => {
                        // Set monotonic anchor
                        base_unix_seconds = Some(unix_seconds);
                        base_instant = Some(Instant::now());

                        // Update current time
                        current_time = unix_seconds
                            .to_offset_datetime(offset)
                            .expect("valid offset datetime");

                        info!(
                            "Clock time set: {} (offset={} minutes)",
                            unix_seconds.as_i64(),
                            offset_minutes
                        );

                        // Store and emit immediate tick with new time
                        resources.current_time.lock(|cell| cell.set(current_time));
                        resources.ticks.signal(());
                    }
                    ClockCommand::SetOffset { minutes } => {
                        offset_minutes = minutes;
                        #[expect(clippy::arithmetic_side_effects, reason = "offset bounds checked")]
                        {
                            offset = UtcOffset::from_whole_seconds(offset_minutes * 60)
                                .unwrap_or(UtcOffset::UTC);
                        }

                        if let Some(anchor) = base_unix_seconds {
                            current_time = anchor
                                .to_offset_datetime(offset)
                                .expect("valid offset datetime");
                        }

                        info!("Clock UTC offset updated to {} minutes", offset_minutes);
                    }
                    ClockCommand::SetTickInterval { interval } => {
                        tick_interval = interval;
                        info!("Clock tick interval updated to {} ms", interval.as_millis());
                    }
                }
            }
        }
    }
}
