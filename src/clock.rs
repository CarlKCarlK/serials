//! A device abstraction that manages timekeeping and emits tick events.
//!
//! See [`Clock`] for usage and examples.

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
use time::{Duration as TimeDuration, OffsetDateTime, UtcOffset};

use crate::Result;
use crate::unix_seconds::UnixSeconds;

// ============================================================================
// Constants
// ============================================================================

/// Duration representing one second.
pub const ONE_SECOND: Duration = Duration::from_secs(1);
/// Duration representing one minute (60 seconds).
pub const ONE_MINUTE: Duration = Duration::from_secs(60);
/// Duration representing one hour (60 minutes).
pub const ONE_HOUR: Duration = Duration::from_secs(3_600);
/// Duration representing one day (24 hours).
pub const ONE_DAY: Duration = Duration::from_secs(86_400);
/// Maximum absolute offset minutes supported by [`UtcOffset`] (< 24h).
const MAX_OFFSET_MINUTES: i32 = (ONE_DAY.as_secs() as i32 / 60) - 1;
/// Fixed-point scale factor for speed multiplier (parts per million).
const SPEED_SCALE_PPM: u64 = 1_000_000;

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
    // Base UTC timestamp in microseconds corresponding to base_instant_ticks (0 = not set)
    base_unix_micros: AtomicI64,
    // Monotonic ticks (microseconds) when base_unix_micros was captured
    base_instant_ticks: AtomicU64,
    // Speed multiplier scaled by SPEED_SCALE_PPM (1.0x = 1_000_000)
    speed_scaled_ppm: AtomicU64,
}

impl ClockStatic {
    fn set_offset_minutes(&self, offset_minutes: i32) {
        self.offset_minutes.store(offset_minutes, Ordering::Relaxed);
    }

    fn set_tick_interval_ms(&self, tick_interval_ms: Option<u64>) {
        let value = tick_interval_ms.unwrap_or(0);
        self.tick_interval_ms.store(value, Ordering::Relaxed);
    }
}

/// A device abstraction that manages time keeping and emits time tick events.
///
/// Pass `Some(duration)` to enable periodic ticks aligned to that interval; use `None` to emit
/// ticks only when time/offset changes.
///
/// # Examples
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// use defmt::info;
/// use embassy_executor::Spawner;
/// use serials::clock::{Clock, ClockStatic, ONE_SECOND, h12_m_s};
/// use serials::unix_seconds::UnixSeconds;
///
/// async fn run_clock(spawner: Spawner) {
///     let _peripherals = embassy_rp::init(Default::default());
///     static CLOCK_STATIC: ClockStatic = Clock::new_static();
///     let clock = Clock::new(&CLOCK_STATIC, -420, Some(ONE_SECOND), spawner); // PDT offset (UTC-7)
///
///     let current_utc_time = UnixSeconds(1_763_647_200); // 2025-11-20 14:00:00 UTC
///     clock.set_utc_time(current_utc_time).await;
///
///     let now_local = clock.now_local();
///     let (hour12, minute, second) = h12_m_s(&now_local);
///     info!("Local time: {:02}:{:02}:{:02} PDT", hour12, minute, second);
///     // Logs: Local time: 07:00:00 PDT
///
///     clock.set_offset_minutes(-480).await; // Switch to PST (UTC-8)
///     let (hour12, minute, second) = h12_m_s(&clock.now_local());
///     info!("Local time: {:02}:{:02}:{:02} PST", hour12, minute, second);
///     // Logs: Local time: 06:00:00 PST
///
///     loop {
///         let tick = clock.wait().await;
///         let (hour12, minute, second) = h12_m_s(&tick);
///         info!("Tick: {:02}:{:02}:{:02}", hour12, minute, second);
///         // Logs: Tick: 06:00:01, Tick: 06:00:02, ...
///     }
/// }
/// ```
pub struct Clock {
    commands: &'static ClockCommands,
    ticks: &'static ClockTicks,
    offset_minutes: &'static AtomicI32,
    tick_interval_ms: &'static AtomicU64,
    base_unix_micros: &'static AtomicI64,
    base_instant_ticks: &'static AtomicU64,
    speed_scaled_ppm: &'static AtomicU64,
}

impl Clock {
    /// Create Clock resources
    #[must_use]
    pub const fn new_static() -> ClockStatic {
        ClockStatic {
            commands: Channel::new(),
            ticks: Signal::new(),
            offset_minutes: AtomicI32::new(0),
            tick_interval_ms: AtomicU64::new(0),
            base_unix_micros: AtomicI64::new(0),
            base_instant_ticks: AtomicU64::new(0),
            speed_scaled_ppm: AtomicU64::new(SPEED_SCALE_PPM),
        }
    }

    /// Create a new Clock device and spawn its task. See [`Clock`] docs for a full example.
    pub fn new(
        clock_static: &'static ClockStatic,
        offset_minutes: i32,
        tick_interval: Option<Duration>,
        spawner: Spawner,
    ) -> Self {
        clock_static.set_offset_minutes(offset_minutes);
        clock_static.set_tick_interval_ms(tick_interval.map(|d| d.as_millis()));
        let token = unwrap!(clock_device_loop(clock_static));
        spawner.spawn(token);
        Self {
            commands: &clock_static.commands,
            ticks: &clock_static.ticks,
            offset_minutes: &clock_static.offset_minutes,
            tick_interval_ms: &clock_static.tick_interval_ms,
            base_unix_micros: &clock_static.base_unix_micros,
            base_instant_ticks: &clock_static.base_instant_ticks,
            speed_scaled_ppm: &clock_static.speed_scaled_ppm,
        }
    }

    /// Wait for and return the next clock tick event. If constructed with `None` tick interval,
    /// ticks occur only when time or offset changes. Passing `Some(duration)` enables periodic
    /// ticks aligned to that interval. See [`Clock`] for usage.
    pub async fn wait(&self) -> OffsetDateTime {
        self.ticks.wait().await;
        self.now_local()
    }

    /// Get the current local time (offset already applied) without waiting for a tick.
    /// Computed from atomics + `Instant::now()` - no async needed.
    pub fn now_local(&self) -> OffsetDateTime {
        let offset_minutes = self.offset_minutes.load(Ordering::Relaxed);
        let base_unix_micros = self.base_unix_micros.load(Ordering::Relaxed);
        core::assert!(
            offset_minutes.unsigned_abs() <= MAX_OFFSET_MINUTES as u32,
            "offset minutes within +/-24h"
        );

        if base_unix_micros == 0 {
            // Time not set - return midnight
            return OffsetDateTime::from_unix_timestamp(0).expect("midnight is valid");
        }

        let base_instant_ticks = self.base_instant_ticks.load(Ordering::Relaxed);
        core::assert!(base_instant_ticks > 0, "base_instant_ticks must be set when time is set");
        let now_ticks = Instant::now().as_ticks();
        core::assert!(now_ticks >= base_instant_ticks);
        let elapsed_ticks = now_ticks - base_instant_ticks;
        let speed_scaled_ppm = self.speed_scaled_ppm.load(Ordering::Relaxed);
        core::assert!(speed_scaled_ppm > 0, "speed multiplier must be positive");
        let scaled_elapsed_micros =
            scale_elapsed_microseconds(elapsed_ticks, speed_scaled_ppm);

        let utc_micros = i128::from(base_unix_micros) + i128::from(scaled_elapsed_micros);
        let utc_seconds = i64::try_from(utc_micros / 1_000_000).expect("utc seconds fits");
        let utc_remainder_micros =
            i64::try_from(utc_micros % 1_000_000).expect("microsecond remainder fits");

        #[expect(
            clippy::arithmetic_side_effects,
            reason = "UtcOffset bounds validate minutes"
        )]
        let offset = UtcOffset::from_whole_seconds(offset_minutes * 60)
            .expect("offset minutes within +/-24h");
        let utc = OffsetDateTime::from_unix_timestamp(utc_seconds).expect("valid utc timestamp")
            + TimeDuration::microseconds(utc_remainder_micros);
        utc.to_offset(offset)
    }

    /// Set the current UTC time. See [`Clock`] docs for usage.
    pub async fn set_utc_time(&self, unix_seconds: UnixSeconds) {
        let unix_seconds = unix_seconds.as_i64();
        let unix_micros = i128::from(unix_seconds) * i128::from(1_000_000);
        let unix_micros = i64::try_from(unix_micros).expect("unix micros fits in i64");
        let now_ticks = Instant::now().as_ticks();

        self.base_unix_micros.store(unix_micros, Ordering::Relaxed);
        self.base_instant_ticks
            .store(now_ticks, Ordering::Relaxed);
        info!("Clock time set: {}", unix_seconds);
        // Notify the device loop to emit a tick
        self.commands.send(ClockCommand::UpdateTicker).await;
    }

    /// Update the UTC offset used for subsequent [`now_local`](Clock::now_local) results and tick events.
    pub async fn set_offset_minutes(&self, minutes: i32) {
        core::assert!(
            minutes.unsigned_abs() <= MAX_OFFSET_MINUTES as u32,
            "offset minutes within +/-24h"
        );
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

    /// Set the tick interval (e.g., `Some(ONE_SECOND)`, `Some(ONE_MINUTE)`, `Some(ONE_HOUR)`).
    /// Use `None` to disable periodic ticks (only emit on time/offset changes). See [`Clock`].
    pub async fn set_tick_interval(&self, interval: Option<Duration>) {
        // Update the atomic immediately
        let interval_ms = interval.map(|d| d.as_millis()).unwrap_or(0);
        self.tick_interval_ms.store(interval_ms, Ordering::Relaxed);
        if interval_ms == 0 {
            info!("Clock tick interval cleared (ticks only on updates)");
        } else {
            info!("Clock tick interval updated to {} ms", interval_ms);
        }
        // Notify device loop to wake up and recalculate sleep duration
        self.commands.send(ClockCommand::UpdateTicker).await;
    }

    /// Update the speed multiplier (1.0 = real time). Changing speed resets the base time to
    /// the current real time so returning to 1.0 resumes the correct clock.
    pub async fn set_speed(&self, speed_multiplier: f32) {
        core::assert!(speed_multiplier.is_finite(), "speed must be finite");
        core::assert!(speed_multiplier > 0.0, "speed must be positive");
        let scaled = speed_multiplier * SPEED_SCALE_PPM as f32 + 0.5;
        core::assert!(scaled.is_finite(), "scaled speed must be finite");
        core::assert!(scaled > 0.0, "scaled speed must be positive");
        core::assert!(
            scaled <= u64::MAX as f32,
            "scaled speed must fit in u64"
        );
        let speed_scaled_ppm = scaled as u64;

        let now_ticks = Instant::now().as_ticks();
        let base_unix_micros = self.base_unix_micros.load(Ordering::Relaxed);
        if base_unix_micros != 0 {
            let base_instant_ticks = self.base_instant_ticks.load(Ordering::Relaxed);
            core::assert!(base_instant_ticks > 0, "base instant must be set when time is set");
            core::assert!(now_ticks >= base_instant_ticks);
            let elapsed_real_ticks = now_ticks - base_instant_ticks;
            let elapsed_real_micros =
                i64::try_from(elapsed_real_ticks).expect("elapsed real micros fits in i64");
            let real_unix_micros =
                i128::from(base_unix_micros) + i128::from(elapsed_real_micros);
            let real_unix_micros =
                i64::try_from(real_unix_micros).expect("real unix micros fits in i64");
            self.base_unix_micros
                .store(real_unix_micros, Ordering::Relaxed);
        }

        self.base_instant_ticks
            .store(now_ticks, Ordering::Relaxed);
        self.speed_scaled_ppm
            .store(speed_scaled_ppm, Ordering::Relaxed);
        info!("Clock speed set: {} ppm", speed_scaled_ppm);
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
    let mut speed_scaled_ppm = resources.speed_scaled_ppm.load(Ordering::Relaxed);
    let offset_minutes = resources.offset_minutes.load(Ordering::Relaxed);

    info!(
        "Clock device started (UTC offset: {} minutes, tick interval: {} ms, speed: {} ppm)",
        offset_minutes, tick_interval_ms, speed_scaled_ppm
    );

    // Helper to calculate duration until next tick boundary
    let sleep_until_boundary = |interval_micros: u64| -> Duration {
        core::assert!(interval_micros > 0);
        let now_ticks = Instant::now().as_ticks();
        let ticks_until_next = interval_micros - (now_ticks % interval_micros);
        Duration::from_micros(ticks_until_next)
    };

    let mut emit_tick = true;
    loop {
        if emit_tick {
            resources.ticks.signal(());
        }
        emit_tick = true;

        if tick_interval_ms == 0 {
            // No periodic ticks; wait for commands to trigger a single tick
            match resources.commands.receive().await {
                ClockCommand::UpdateTicker => {
                    tick_interval_ms = resources.tick_interval_ms.load(Ordering::Relaxed);
                    speed_scaled_ppm = resources.speed_scaled_ppm.load(Ordering::Relaxed);
                    // emit_tick remains true for next loop iteration
                }
            }
            continue;
        }

        // Calculate sleep duration aligned to tick boundary
        let interval_micros =
            scaled_interval_microseconds(tick_interval_ms, speed_scaled_ppm);
        let sleep_duration = sleep_until_boundary(interval_micros);

        // Wait for either tick interval or a command
        match select(Timer::after(sleep_duration), resources.commands.receive()).await {
            Either::First(_) => {
                // Timer elapsed - tick occurred, loop will signal again
            }
            Either::Second(ClockCommand::UpdateTicker) => {
                tick_interval_ms = resources.tick_interval_ms.load(Ordering::Relaxed);
                speed_scaled_ppm = resources.speed_scaled_ppm.load(Ordering::Relaxed);
                emit_tick = true;
            }
        }
    }
}

fn scaled_interval_microseconds(interval_ms: u64, speed_scaled_ppm: u64) -> u64 {
    core::assert!(interval_ms > 0, "interval must be positive");
    core::assert!(speed_scaled_ppm > 0, "speed must be positive");
    let interval_micros = interval_ms
        .checked_mul(1_000)
        .expect("interval micros fits in u64");
    let scaled = u128::from(interval_micros) * u128::from(SPEED_SCALE_PPM)
        / u128::from(speed_scaled_ppm);
    let scaled = u64::try_from(scaled).expect("scaled interval fits in u64");
    core::assert!(scaled > 0, "scaled interval must be positive");
    scaled
}

fn scale_elapsed_microseconds(elapsed_ticks: u64, speed_scaled_ppm: u64) -> i64 {
    core::assert!(speed_scaled_ppm > 0, "speed must be positive");
    let scaled = u128::from(elapsed_ticks) * u128::from(speed_scaled_ppm)
        / u128::from(SPEED_SCALE_PPM);
    i64::try_from(scaled).expect("scaled elapsed fits in i64")
}
