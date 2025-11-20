//! A device abstraction that manages timekeeping and emits tick events.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use core::sync::atomic::{AtomicI32, Ordering};
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use portable_atomic::{AtomicI64, AtomicU64};
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

/// Extract hour (12-hour format), minute, second from OffsetDateTime
pub fn h12_m_s(dt: &OffsetDateTime) -> (u8, u8, u8) {
    let hour_24 = dt.hour() as u8;
    let hour_12 = match hour_24 {
        0 => 12,
        1..=12 => hour_24,
        _ => hour_24 - 12,
    };
    let minute = dt.minute() as u8;
    let second = dt.second() as u8;
    (hour_12, minute, second)
}

/// Commands sent to the clock device.
enum ClockCommand {
    /// Emit a tick notification (used when time/offset changes)
    UpdateTicker,
}

// ============================================================================
// Clock Virtual Device
// ============================================================================

/// Channel type for clock commands.
type ClockCommands = Channel<CriticalSectionRawMutex, ClockCommand, 4>;
/// Signal type for clock tick notifications.
type ClockTicks = Signal<CriticalSectionRawMutex, ()>;

/// Resources needed by Clock device
pub struct ClockStatic {
    commands: ClockCommands,
    ticks: ClockTicks,
    offset_minutes: AtomicI32,
    tick_interval_ms: AtomicU64,
    // Unix timestamp when the processor booted (0 = not set)
    boot_unix_seconds: AtomicI64,
}

impl ClockStatic {
    fn set_offset_minutes(&self, offset_minutes: i32) {
        self.offset_minutes.store(offset_minutes, Ordering::Relaxed);
    }

    fn set_tick_interval_ms(&self, tick_interval_ms: u64) {
        self.tick_interval_ms
            .store(tick_interval_ms, Ordering::Relaxed);
    }
}

/// A device abstraction that manages time keeping and emits time tick events.
pub struct Clock {
    commands: &'static ClockCommands,
    ticks: &'static ClockTicks,
    offset_minutes: &'static AtomicI32,
    tick_interval_ms: &'static AtomicU64,
    boot_unix_seconds: &'static AtomicI64,
}

impl Clock {
    /// Create Clock resources
    #[must_use]
    pub const fn new_static() -> ClockStatic {
        ClockStatic {
            commands: Channel::new(),
            ticks: Signal::new(),
            offset_minutes: AtomicI32::new(0),
            tick_interval_ms: AtomicU64::new(1000),
            boot_unix_seconds: AtomicI64::new(0),
        }
    }

    /// Create a new Clock device and spawn its task
    pub fn new(clock_static: &'static ClockStatic, offset_minutes: i32, spawner: Spawner) -> Self {
        clock_static.set_offset_minutes(offset_minutes);
        clock_static.set_tick_interval_ms(ONE_SECOND.as_millis());
        let token = unwrap!(clock_device_loop(clock_static));
        spawner.spawn(token);
        Self {
            commands: &clock_static.commands,
            ticks: &clock_static.ticks,
            offset_minutes: &clock_static.offset_minutes,
            tick_interval_ms: &clock_static.tick_interval_ms,
            boot_unix_seconds: &clock_static.boot_unix_seconds,
        }
    }

    /// Wait for and return the next clock tick event
    pub async fn wait(&self) -> OffsetDateTime {
        self.ticks.wait().await;
        self.current_time()
    }

    /// Get the current time without waiting for a tick.
    /// Computed from atomics + Instant::now() - no async needed.
    pub fn current_time(&self) -> OffsetDateTime {
        let _boot_unix = self.boot_unix_seconds.load(Ordering::Relaxed);
        let offset_minutes = self.offset_minutes.load(Ordering::Relaxed);
        self.current_time_with_offset(offset_minutes)
    }

    /// Get the current time with a specific offset (useful when offset is changing).
    pub fn current_time_with_offset(&self, offset_minutes: i32) -> OffsetDateTime {
        let boot_unix = self.boot_unix_seconds.load(Ordering::Relaxed);

        if boot_unix == 0 {
            // Time not set - return midnight
            return OffsetDateTime::from_unix_timestamp(0).expect("midnight is valid");
        }

        // Current time = boot time + time since boot + timezone offset
        let elapsed_secs = Instant::now().as_secs();
        #[expect(clippy::arithmetic_side_effects, reason = "saturating_add used")]
        let utc_unix_seconds = boot_unix.saturating_add(elapsed_secs as i64);

        // Apply timezone offset to get local time
        #[expect(clippy::arithmetic_side_effects, reason = "offset bounds checked")]
        let offset_seconds = i64::from(offset_minutes) * 60;
        #[expect(clippy::arithmetic_side_effects, reason = "saturating_add used")]
        let local_unix_seconds = utc_unix_seconds.saturating_add(offset_seconds);

        let unix_seconds = UnixSeconds(local_unix_seconds);
        unix_seconds
            .to_offset_datetime(UtcOffset::UTC)
            .expect("valid offset datetime")
    }

    /// Send a command to set the time
    pub async fn set_time(&self, unix_seconds: UnixSeconds) {
        // Calculate and update boot time immediately
        let uptime_secs = Instant::now().as_secs();
        let boot_unix = unix_seconds.as_i64().saturating_sub(uptime_secs as i64);
        self.boot_unix_seconds.store(boot_unix, Ordering::Relaxed);
        info!(
            "Clock time set: {} (boot time: {})",
            unix_seconds.as_i64(),
            boot_unix
        );
        // Notify the device loop to emit a tick
        self.commands.send(ClockCommand::UpdateTicker).await;
    }

    /// Update the UTC offset used for subsequent ticks.
    pub async fn set_offset_minutes(&self, minutes: i32) {
        // Update the atomic immediately
        self.offset_minutes.store(minutes, Ordering::Relaxed);
        info!("Clock UTC offset updated to {} minutes", minutes);
        // Notify the device loop to emit a tick
        self.commands.send(ClockCommand::UpdateTicker).await;
    }

    /// Get the current UTC offset in minutes.
    pub fn offset_minutes(&self) -> i32 {
        self.offset_minutes.load(Ordering::Relaxed)
    }

    /// Set the tick interval (e.g., Duration::from_secs(1), Duration::from_secs(60)).
    /// The clock will emit events aligned to boundaries (top of second, top of minute, etc.).
    pub async fn set_tick_interval(&self, interval: Duration) {
        // Update the atomic immediately
        let interval_ms = interval.as_millis();
        self.tick_interval_ms.store(interval_ms, Ordering::Relaxed);
        info!("Clock tick interval updated to {} ms", interval_ms);
        // Notify device loop to wake up and recalculate sleep duration
        self.commands.send(ClockCommand::UpdateTicker).await;
    }
}

#[embassy_executor::task]
async fn clock_device_loop(resources: &'static ClockStatic) -> ! {
    let err = inner_clock_device_loop(resources).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_clock_device_loop(resources: &'static ClockStatic) -> Result<Infallible> {
    // Local loop variables
    let mut tick_interval_ms = resources.tick_interval_ms.load(Ordering::Relaxed);
    let offset_minutes = resources.offset_minutes.load(Ordering::Relaxed);

    info!(
        "Clock device started (UTC offset: {} minutes, tick interval: {} ms)",
        offset_minutes, tick_interval_ms
    );

    // Helper to calculate duration until next tick boundary
    let sleep_until_boundary = |interval_ms: u64| -> Duration {
        let now_ticks = Instant::now().as_ticks();
        let interval_ticks = interval_ms * 1000; // ms to microseconds
        let ticks_until_next = interval_ticks - (now_ticks % interval_ticks);
        Duration::from_micros(ticks_until_next)
    };

    loop {
        // Emit tick notification
        resources.ticks.signal(());

        // Calculate sleep duration aligned to tick boundary
        let sleep_duration = sleep_until_boundary(tick_interval_ms);

        // Wait for either tick interval or a command
        match select(Timer::after(sleep_duration), resources.commands.receive()).await {
            Either::First(_) => {
                // Timer elapsed - tick occurred, loop will signal again
            }
            Either::Second(ClockCommand::UpdateTicker) => {
                // Caller updated atomics and wants an immediate tick
                // Also read the latest tick interval in case it changed
                tick_interval_ms = resources.tick_interval_ms.load(Ordering::Relaxed);
                resources.ticks.signal(());
            }
        }
    }
}
