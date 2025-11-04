//! Log Time - Simple NTP time synchronization with logging
//!
//! This example demonstrates WiFi-based time synchronization using the TimeSync virtual device.
//! It connects to WiFi, syncs with an NTP server, and logs time events to the debug console.
//!
//! Run with:
//!   - Pico 1 W: `cargo log_time_1w`
//!   - Pico 2 W: `cargo log_time_2w`

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use lib::{Clock, ClockNotifier, Result, TimeSync, TimeSyncEvent, TimeSyncNotifier};
use panic_probe as _;

// ============================================================================
// Main
// ============================================================================

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting Log Time Example");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Create Clock device (starts ticking immediately)
    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let clock = Clock::new(&CLOCK_NOTIFIER, spawner);

    // Create TimeSync virtual device (creates WiFi internally and starts syncing)
    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        p.PIN_23,  // WiFi power enable
        p.PIN_25,  // WiFi SPI chip select
        p.PIO0,    // WiFi PIO block for SPI
        p.PIN_24,  // WiFi SPI MOSI
        p.PIN_29,  // WiFi SPI CLK
        p.DMA_CH0, // WiFi DMA channel for SPI
        spawner,
    );

    info!("WiFi and time sync initialized, waiting for events...");

    // Main event loop - log time on every tick and handle sync events
    loop {
        match select(clock.wait(), time_sync.wait()).await {
            // On every clock tick, log the current time
            Either::First(time_info) => {
                let dt = time_info.datetime;
                info!(
                    "Current time: {:04}-{:02}-{:02} {:02}:{:02}:{:02} (state: {})",
                    dt.year(),
                    u8::from(dt.month()),
                    dt.day(),
                    dt.hour(),
                    dt.minute(),
                    dt.second(),
                    match time_info.state {
                        lib::ClockState::NotSet => "NOT SET",
                        lib::ClockState::Synced => "SYNCED",
                    }
                );
            }

            // On time sync success, update the clock
            Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                info!("Time sync SUCCESS: unix_seconds={}", unix_seconds.as_i64());
                clock.set_time(unix_seconds).await;
            }

            // On time sync failure, just log the error
            Either::Second(TimeSyncEvent::Failed(err)) => {
                info!("Time sync FAILED: {}", err);
            }
        }
    }
}
