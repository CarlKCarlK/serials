//! LCD Clock - Event-driven time display with WiFi sync

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::fmt;
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use heapless::String;
use panic_probe as _;
use serials::{Error, Result};
use serials::char_lcd::{CharLcd, CharLcdStatic};
use serials::clock::{Clock, ClockStatic, ONE_SECOND};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use serials::wifi_setup::fields::{TimezoneField, TimezoneFieldStatic};
use serials::wifi_setup::{WifiSetup, WifiSetupStatic};

// ============================================================================
// Main Orchestrator
// ============================================================================

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("Starting LCD Clock with WiFi");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Initialize CharLcd
    static CHAR_LCD_STATIC: CharLcdStatic = CharLcd::new_static();
    let char_lcd = CharLcd::new(&CHAR_LCD_STATIC, p.I2C0, p.PIN_5, p.PIN_4, spawner)?;

    // Use two blocks of flash storage: Wi-Fi credentials + timezone
    static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
    let [wifi_credentials_flash_block, timezone_flash_block] =
        FlashArray::new(&FLASH_STATIC, p.FLASH)?;

    // Define timezone field for captive portal
    static TIMEZONE_FIELD_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
    let timezone_field = TimezoneField::new(&TIMEZONE_FIELD_STATIC, timezone_flash_block);

    // Set up WiFi via captive portal
    static WIFI_SETUP_STATIC: WifiSetupStatic = WifiSetup::new_static();
    let wifi_setup = WifiSetup::new(
        &WIFI_SETUP_STATIC,
        p.PIN_23,  // CYW43 power
        p.PIN_25,  // CYW43 chip select
        p.PIO0,    // CYW43 PIO interface
        p.PIN_24,  // CYW43 clock
        p.PIN_29,  // CYW43 data pin
        p.DMA_CH0, // CYW43 DMA channel
        wifi_credentials_flash_block,
        p.PIN_13, // Reset button pin
        "PicoClock",
        [timezone_field],
        spawner,
    )?;

    // Connect to WiFi
    let (stack, _button) = wifi_setup.connect(spawner, |_event| async move {}).await?;

    // Create Clock device with timezone from WiFi portal
    // cmk offset must be set or return error
    let timezone_offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
    static CLOCK_STATIC: ClockStatic = Clock::new_static();
    let clock = Clock::new(
        &CLOCK_STATIC,
        timezone_offset_minutes,
        Some(ONE_SECOND),
        spawner,
    );

    // Create TimeSync with network stack
    static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    let time_sync = TimeSync::new(&TIME_SYNC_STATIC, stack, spawner);

    info!("Entering main event loop");

    // Main orchestrator loop - owns LCD and displays clock/sync events
    loop {
        match select(clock.wait_for_tick(), time_sync.wait_for_sync()).await {
            // On every tick event, update the LCD display
            Either::First(time_info) => {
                let mut text = String::<64>::new();
                let (hour12, am_pm) = if time_info.hour() == 0 {
                    (12, "AM")
                } else if time_info.hour() < 12 {
                    (time_info.hour(), "AM")
                } else if time_info.hour() == 12 {
                    (12, "PM")
                } else {
                    #[expect(clippy::arithmetic_side_effects, reason = "hour guaranteed 13-23")]
                    {
                        (time_info.hour() - 12, "PM")
                    }
                };
                fmt::Write::write_fmt(
                    &mut text,
                    format_args!(
                        "{:2}:{:02}:{:02} {}\n{:04}-{:02}-{:02}",
                        hour12,
                        time_info.minute(),
                        time_info.second(),
                        am_pm,
                        time_info.year(),
                        u8::from(time_info.month()),
                        time_info.day()
                    ),
                )
                .map_err(|_| Error::FormatError)?;
                char_lcd.display(text, 0).await;
            }

            // On time sync events, set clock and display status
            Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                info!("Sync successful: unix_seconds={}", unix_seconds.as_i64());
                clock.set_utc_time(unix_seconds).await;
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
