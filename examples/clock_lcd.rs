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
use panic_probe as _;
use serials::Result;
use serials::char_lcd::{CharLcd, CharLcdStatic};
use serials::clock::{Clock, ClockStatic};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::time_sync_old::{TimeSync, TimeSyncEvent, TimeSyncStatic};

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
    static CHAR_LCD_STATIC: CharLcdStatic = CharLcd::new_static();
    let char_lcd = CharLcd::new(&CHAR_LCD_STATIC, p.I2C0, p.PIN_5, p.PIN_4, spawner)?;

    // Create Clock device (starts ticking immediately)
    const DEFAULT_UTC_OFFSET_MINUTES: i32 = 0;
    static CLOCK_STATIC: ClockStatic = Clock::new_static();
    let clock = Clock::new(&CLOCK_STATIC, spawner);
    clock
        .set_utc_offset_minutes(DEFAULT_UTC_OFFSET_MINUTES)
        .await;

    // Create TimeSync virtual device (creates WiFi internally)
    static TIME_SYNC: TimeSyncStatic = TimeSync::new_static();
    #[cfg(feature = "wifi")]
    let time_sync = {
        static WIFI_FLASH_STATIC: FlashArrayStatic = FlashArray::<1>::new_static();
        let [wifi_block] = FlashArray::new(&WIFI_FLASH_STATIC, p.FLASH)?;
        TimeSync::new(
            &TIME_SYNC,
            p.PIN_23,   // WiFi power enable
            p.PIN_25,   // WiFi SPI chip select
            p.PIO0,     // WiFi PIO block for SPI
            p.PIN_24,   // WiFi SPI MOSI
            p.PIN_29,   // WiFi SPI CLK
            p.DMA_CH0,  // WiFi DMA channel for SPI
            wifi_block, // Flash partition for WiFi credentials
            serials::wifi::DEFAULT_CAPTIVE_PORTAL_SSID,
            spawner,
        )
    };
    #[cfg(not(feature = "wifi"))]
    let time_sync = TimeSync::new(&TIME_SYNC, spawner);

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
