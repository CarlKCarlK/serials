//! LCD Clock - Event-driven time display

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use heapless::String;
use lib::{
    CharLcd, Clock, ClockNotifier, LcdChannel, Result, TimeSync, TimeSyncEvent,
    TimeSyncNotifier,
};
use panic_probe as _;

// ============================================================================
// Main Orchestrator
// ============================================================================

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting LCD Clock (Event-Driven)");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Initialize LCD
    static LCD_CHANNEL: LcdChannel = CharLcd::channel();
    let lcd = CharLcd::new(p.I2C0, p.PIN_5, p.PIN_4, &LCD_CHANNEL, spawner)?;

    // Create Clock device (starts ticking immediately)
    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let clock = Clock::new(&CLOCK_NOTIFIER, spawner);

    // Create TimeSync virtual device (creates WiFi internally)
    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        p.PIN_23,      // WiFi power enable
        p.PIN_25,      // WiFi SPI chip select
        p.PIO0,        // WiFi PIO block for SPI
        p.PIN_24,      // WiFi SPI MOSI
        p.PIN_29,      // WiFi SPI CLK
        p.DMA_CH0,     // WiFi DMA channel for SPI
        spawner,
    );

    info!("Entering main event loop");

    // Main orchestrator loop - owns LCD and displays clock/sync events
    loop {
        match select(clock.next_event(), time_sync.next_event()).await {
            // On every tick event, update the LCD display
            Either::First(time_info) => {
                let text = Clock::format_display(&time_info)?;
                lcd.display(text, 0);
            }
            
            // On time sync events, set clock and display status
            Either::Second(TimeSyncEvent::SyncSuccess { unix_seconds }) => {
                info!("Sync successful: unix_seconds={}", unix_seconds.as_i64());
                clock.set_time(unix_seconds).await;
                lcd.display(String::<64>::try_from("Synced!").unwrap(), 800);
            }

            // On sync failure, display error
            Either::Second(TimeSyncEvent::SyncFailed(err)) => {
                info!("Sync failed: {}", err);
                lcd.display(String::<64>::try_from("Sync failed").unwrap(), 800);
            }
        }
    }
}
