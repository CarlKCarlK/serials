//! Wi-Fi enabled 4-digit clock that provisions credentials through `WifiAuto`.
//!
//! This example demonstrates how to pair the shared captive-portal workflow with the
//! `ClockLed4` state machine. The `WifiAuto` helper owns Wi-Fi onboarding while the
//! clock display reflects progress and, once connected, continues handling user input.

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{self, Level};
use panic_probe as _;
use serials::Result;
use serials::clock_led4::state::ClockLed4State;
use serials::clock_led4::{ClockLed4 as Clock, ClockLed4Static as ClockStatic};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::led4::OutputArray;
use serials::time_sync::{TimeSync, TimeSyncStatic};
use serials::wifi::DEFAULT_AP_SSID;
use serials::wifi_auto::fields::{TextField, TextFieldStatic, TimezoneField, TimezoneFieldStatic};
use serials::wifi_auto::{WifiAuto, WifiAutoEvent, WifiAutoStatic};

const NICKNAME_MAX_LEN: usize = 32;
const DEFAULT_NICKNAME: &str = "PicoClock";

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting Wi-Fi 4-digit clock (WifiAuto)");
    let peripherals = embassy_rp::init(Default::default());

    // Initialize flash storage: Wi-Fi credentials + timezone + nickname.
    static FLASH_STATIC: FlashArrayStatic = FlashArray::<3>::new_static();
    let [wifi_credentials_flash, timezone_flash, nickname_flash] =
        FlashArray::new(&FLASH_STATIC, peripherals.FLASH)?;

    static TIMEZONE_FIELD_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
    let timezone_field = TimezoneField::new(&TIMEZONE_FIELD_STATIC, timezone_flash);
    let timezone_offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);

    static NICKNAME_FIELD_STATIC: TextFieldStatic<NICKNAME_MAX_LEN> = TextField::new_static();
    let nickname_field = TextField::new(
        &NICKNAME_FIELD_STATIC,
        nickname_flash,
        "nickname",
        "Nickname",
        DEFAULT_NICKNAME,
    );

    // Initialize LED (unused but kept for board compatibility).
    let mut led = gpio::Output::new(peripherals.PIN_0, Level::Low);
    led.set_low();

    // Initialize display pins.
    let cells = OutputArray::new([
        gpio::Output::new(peripherals.PIN_1, Level::High),
        gpio::Output::new(peripherals.PIN_2, Level::High),
        gpio::Output::new(peripherals.PIN_3, Level::High),
        gpio::Output::new(peripherals.PIN_4, Level::High),
    ]);

    let segments = OutputArray::new([
        gpio::Output::new(peripherals.PIN_5, Level::Low),
        gpio::Output::new(peripherals.PIN_6, Level::Low),
        gpio::Output::new(peripherals.PIN_7, Level::Low),
        gpio::Output::new(peripherals.PIN_8, Level::Low),
        gpio::Output::new(peripherals.PIN_9, Level::Low),
        gpio::Output::new(peripherals.PIN_10, Level::Low),
        gpio::Output::new(peripherals.PIN_11, Level::Low),
        gpio::Output::new(peripherals.PIN_12, Level::Low),
    ]);

    static CLOCK_STATIC: ClockStatic = Clock::new_static();
    let mut clock = Clock::new(
        cells,
        segments,
        &CLOCK_STATIC,
        spawner,
        timezone_offset_minutes,
    )?;

    static WIFI_AUTO_STATIC: WifiAutoStatic = WifiAuto::new_static();
    let wifi_auto = WifiAuto::new(
        &WIFI_AUTO_STATIC,
        peripherals.PIN_23,     // CYW43 power
        peripherals.PIN_25,     // CYW43 chip select
        peripherals.PIO0,       // CYW43 PIO interface
        peripherals.PIN_24,     // CYW43 clock
        peripherals.PIN_29,     // CYW43 data pin
        peripherals.DMA_CH0,    // CYW43 DMA channel
        wifi_credentials_flash, // Flash block storing Wi-Fi creds
        peripherals.PIN_13,     // Reset button pin
        DEFAULT_AP_SSID,        // Captive-portal SSID
        [timezone_field, nickname_field],
        spawner,
    )?;

    // Drive the display with WifiAuto events while onboarding runs.
    let clock_ref = &clock;
    let (stack, mut button) = wifi_auto
        .ensure_connected_with_async_ui(spawner, move |event| {
            let clock_ref = clock_ref;
            async move {
                match event {
                    WifiAutoEvent::CaptivePortalReady => {
                        clock_ref.show_access_point_setup().await;
                    }
                    WifiAutoEvent::ClientConnecting { .. } => {
                        clock_ref.set_state(ClockLed4State::Connecting).await;
                    }
                    WifiAutoEvent::Connected => {
                        clock_ref.set_state(ClockLed4State::HoursMinutes).await;
                    }
                }
            }
        })
        .await?;

    let timezone_offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
    clock.set_utc_offset_minutes(timezone_offset_minutes).await;

    static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    let time_sync = TimeSync::new_from_stack(&TIME_SYNC_STATIC, stack, spawner);

    let mut clock_state = ClockLed4State::HoursMinutes;
    let mut persisted_offset = clock.utc_offset_minutes();

    loop {
        clock_state = clock_state
            .execute(&mut clock, &mut button, time_sync)
            .await;

        let current_offset = clock.utc_offset_minutes();
        if current_offset != persisted_offset {
            timezone_field.set_offset_minutes(current_offset)?;
            persisted_offset = current_offset;
        }
    }
}
