//! Log Time - WiFi Configuration and NTP Time Synchronization
//!
//! This example demonstrates a complete WiFi configuration workflow:
//! 1. Starts in AP mode for WiFi credential collection
//! 2. User connects to "PicoConfig" AP and enters their WiFi credentials via web interface
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
use defmt::{info, unwrap};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use lib::{
    Clock, ClockNotifier, Result, TimeSync, TimeSyncEvent, TimeSyncNotifier,
    WifiMode, collect_wifi_credentials, dns_server_task,
};
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
    info!("Starting Log Time Example with WiFi Configuration");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Create Clock device (starts ticking immediately)
    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let clock = Clock::new(&CLOCK_NOTIFIER, spawner);

    // Determine WiFi mode based on compile-time flag
    // For now, we'll start in AP mode to demonstrate the configuration flow
    // In a real application, you'd check if credentials are saved
    let wifi_mode = WifiMode::AccessPoint;
    
    info!("Starting WiFi in AP mode for configuration...");

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
        wifi_mode,
        spawner,
    );

    if wifi_mode == WifiMode::AccessPoint {
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
        info!("Connect to WiFi 'PicoConfig' and open browser to http://192.168.4.1");
        info!("(Android/iOS should show 'Sign in to network' notification)");
        info!("");
        info!("==========================================================");
        info!("WAITING FOR CONFIGURATION");
        info!("==========================================================");
        info!("");
        
        // Collect credentials from user via web interface
        let credentials = collect_wifi_credentials(stack, spawner).await?;
        
        info!("==========================================================");
        info!("CREDENTIALS RECEIVED!");
        info!("==========================================================");
        info!("SSID: {}", credentials.ssid);
        info!("Password: [hidden]");
        info!("");
        info!("To connect to this network:");
        info!("1. Set environment variables in .env file:");
        info!("   WIFI_SSID=\"{}\"", credentials.ssid);
        info!("   WIFI_PASS=\"<your password>\"");
        info!("2. Power cycle the device to restart in client mode");
        info!("");
        info!("TODO: Implement automatic mode switching without restart");
        info!("==========================================================");
        
        // For now, just stay in AP mode
        // TODO: Switch to client mode without restart
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
