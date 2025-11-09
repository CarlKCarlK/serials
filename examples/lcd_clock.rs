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
use lib::Result;
use lib::char_lcd::{CharLcd, CharLcdNotifier};
use lib::clock::{Clock, ClockNotifier};
use lib::time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
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

    // Initialize CharLcd
    static CHAR_LCD_NOTIFIER: CharLcdNotifier = CharLcd::notifier();
    let char_lcd = CharLcd::new(p.I2C0, p.PIN_5, p.PIN_4, &CHAR_LCD_NOTIFIER, spawner)?;

    // Create Clock device (starts ticking immediately)
    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let clock = Clock::new(&CLOCK_NOTIFIER, spawner);

    // Create TimeSync virtual device (creates WiFi internally)
    static TIME_SYNC: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC, p.PIN_23,  // WiFi power enable
        p.PIN_25,  // WiFi SPI chip select
        p.PIO0,    // WiFi PIO block for SPI
        p.PIN_24,  // WiFi SPI MOSI
        p.PIN_29,  // WiFi SPI CLK
        p.DMA_CH0, // WiFi DMA channel for SPI
        spawner,
    );

    info!("Entering main event loop");

    // Main orchestrator loop - owns LCD and displays clock/sync events
    loop {
        match select(clock.wait(), time_sync.wait()).await {
            // On every tick event, update the LCD display
            Either::First(time_info) => {
                let text = Clock::format_display(&time_info)?;
                char_lcd.display(text, 0).await;
            }

            // On time sync events, set clock and display status
            Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                info!("Sync successful: unix_seconds={}", unix_seconds.as_i64());
                clock.set_time(unix_seconds).await;
                char_lcd
                    .display(String::<64>::try_from("Synced!").unwrap(), 800)
                    .await;
            }

            // On sync failure, display error message for at least 8/10th of a second
            Either::Second(TimeSyncEvent::Failed(err)) => {
                info!("Sync failed: {}", err);
                char_lcd
                    .display(String::<64>::try_from("Sync failed").unwrap(), 800)
                    .await;
            }
        }
    }
}
