//! Console Clock - WiFi-synced time logging to console
//!
//! This example demonstrates WiFi connection with auto-provisioning
//! and logs time sync events to the console.

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use panic_probe as _;
use serials::Result;
use serials::clock::{Clock, ClockStatic, ONE_SECOND};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use serials::wifi_setup::WifiSetupEvent;
use serials::wifi_setup::fields::{TimezoneField, TimezoneFieldStatic};
use serials::wifi_setup::{WifiSetup, WifiSetupStatic};

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("Starting Console Clock with WiFi");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

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
    let (stack, _button) = wifi_setup
        .connect(spawner, |event| async move {
            match event {
                WifiSetupEvent::CaptivePortalReady => {
                    info!("Captive portal ready - connect to WiFi network");
                }
                WifiSetupEvent::Connecting {
                    try_index,
                    try_count,
                } => {
                    info!(
                        "Connecting to WiFi (attempt {} of {})...",
                        try_index + 1,
                        try_count
                    );
                }
                WifiSetupEvent::Connected => {
                    info!("WiFi connected successfully!");
                }
            }
        })
        .await?;

    // Create TimeSync with network stack
    static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    let time_sync = TimeSync::new(&TIME_SYNC_STATIC, stack, spawner);

    // Create Clock device with timezone from WiFi portal
    let timezone_offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
    static CLOCK_STATIC: ClockStatic = Clock::new_static();
    let clock = Clock::new(&CLOCK_STATIC, timezone_offset_minutes, ONE_SECOND, spawner);

    info!("WiFi connected, entering event loop");

    // Main event loop - log time on every tick and handle sync events
    loop {
        match select(clock.wait(), time_sync.wait()).await {
            // On every clock tick, log the current time
            Either::First(time_info) => {
                info!(
                    "Current time: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    time_info.year(),
                    u8::from(time_info.month()),
                    time_info.day(),
                    time_info.hour(),
                    time_info.minute(),
                    time_info.second(),
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
