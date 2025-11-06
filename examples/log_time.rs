//! Log Time - WiFi Configuration and NTP Time Synchronization
//!
//! This example demonstrates a complete WiFi configuration workflow:
//! 1. Starts in AP mode for WiFi credential collection
//! 2. User connects to "PicoClockConfig" AP and enters their WiFi credentials via web interface
//! 3. Switches to client mode and connects to the configured network
//! 4. Syncs time with NTP server and logs time events
//!
//! NOTE: This example requires device restart to switch from AP to client mode.
//! A future version may support runtime mode switching.
//!
//! Run with:
//!   - Pico 1 W: `cargo log_time_1w`
//!   - Pico 2 W: `cargo log_time_2w`
//!
//! TODOs:
//! - List local WiFi networks for user selection
//! - Save credentials between reboots (but not forever)

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use cortex_m::peripheral::SCB;
use defmt::{info, unwrap};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::flash::{Blocking, Flash};
use embassy_time::Timer;
use lib::credential_store::INTERNAL_FLASH_SIZE;
use lib::{
    Clock, ClockNotifier, Result, TimeSync, TimeSyncEvent, TimeSyncNotifier, WifiMode,
    collect_wifi_credentials, credential_store, dns_server_task, save_timezone_offset,
};
use panic_probe as _;
use static_cell::StaticCell;

// ============================================================================
// Main
// ============================================================================

static FLASH_STORAGE: StaticCell<
    Flash<'static, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
> = StaticCell::new();

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting Log Time Example with WiFi Configuration");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Prepare flash storage for WiFi credentials
    let flash = FLASH_STORAGE.init(Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(
        p.FLASH,
    ));
    let stored_credentials = credential_store::load(&mut *flash)?;

    // Create Clock device (starts ticking immediately)
    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let clock = Clock::new(&CLOCK_NOTIFIER, spawner);

    // Determine WiFi mode based on persisted credentials
    let wifi_mode = if let Some(credentials) = stored_credentials {
        info!(
            "Stored WiFi credentials found for SSID: {}",
            credentials.ssid
        );
        WifiMode::ClientConfigured(credentials)
    } else {
        info!("No stored WiFi credentials - starting configuration access point");
        WifiMode::AccessPoint
    };

    // Create TimeSync virtual device (creates WiFi internally)
    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        p.PIN_23,  // WiFi power enable
        p.PIN_25,  // WiFi SPI chip select
        p.PIO0,    // WiFi PIO block for SPI
        p.PIN_24,  // WiFi SPI MOSI
        p.PIN_29,  // WiFi SPI CLK
        p.DMA_CH0, // WiFi DMA channel for SPI
        wifi_mode.clone(),
        spawner,
    );

    if matches!(wifi_mode, WifiMode::AccessPoint) {
        info!("Starting WiFi in AP mode for configuration...");
        info!("WiFi AP mode - starting HTTP configuration server...");

        // Wait for WiFi stack to be ready
        let stack = time_sync.wifi().stack().await;
        info!("Network stack available for AP mode");

        // Spawn DNS server for captive portal detection
        // This makes Android/iOS show "Sign in to network" notification
        let ap_ip = embassy_net::Ipv4Address::new(192, 168, 4, 1);
        let dns_token = unwrap!(dns_server_task(stack, ap_ip));
        spawner.spawn(dns_token);
        info!("DNS server started - captive portal detection enabled");

        info!("Collecting WiFi credentials from web interface...");
        info!("Connect to WiFi 'PicoClockConfig' and open browser to http://192.168.4.1");
        info!("(Android/iOS should show 'Sign in to network' notification)");
        info!("");
        info!("==========================================================");
        info!("WAITING FOR CONFIGURATION");
        info!("==========================================================");
        info!("");

        // Collect credentials from user via web interface
        let submission = collect_wifi_credentials(stack, spawner).await?;

        info!("==========================================================");
        info!("CREDENTIALS RECEIVED!");
        info!("==========================================================");
        info!("SSID: {}", submission.credentials.ssid);
        info!("Password: [hidden]");
        info!(
            "Timezone offset (minutes): {}",
            submission.timezone_offset_minutes
        );
        info!("");
        info!("Persisting credentials to flash storage...");
        credential_store::save(&mut *flash, &submission.credentials)?;
        save_timezone_offset(&mut *flash, submission.timezone_offset_minutes)?;
        info!("Device will reboot and connect using the stored credentials.");
        info!("==========================================================");

        Timer::after_millis(750).await;
        SCB::sys_reset();
    } else {
        info!("Using stored WiFi credentials - starting client mode directly");
    }

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
