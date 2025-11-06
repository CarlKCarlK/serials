//! Port of the `clock-wifi` example that now reuses the shared Wi-Fi onboarding and
//! NTP synchronisation flow from `examples/log_time.rs`.
//!
//! The clock starts at `12:00`, launches the captive-portal workflow if credentials
//! are missing, then keeps the display updated with hourly NTP refreshes. Buttons
//! continue to toggle between `HH:MM` and `MM:SS` once synchronised.

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use cortex_m::peripheral::SCB;
use defmt::{info, unwrap};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_net::Ipv4Address;
use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::gpio::{self, Level};
use embassy_time::Timer;
use lib::credential_store::INTERNAL_FLASH_SIZE;
use lib::cwf::{
    Clock, ClockNotifier, ClockState, OutputArray, TimeSync, TimeSyncNotifier,
    current_utc_offset_minutes, set_initial_utc_offset_minutes,
};
use lib::{
    Button, Result, WifiMode, clear_timezone_offset, collect_wifi_credentials, credential_store,
    dns_server_task, load_timezone_offset, save_timezone_offset,
};
use panic_probe as _;
use static_cell::StaticCell;

static FLASH_STORAGE: StaticCell<
    Flash<'static, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
> = StaticCell::new();

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting Wi-Fi 4-digit clock");

    let peripherals = embassy_rp::init(Default::default());

    let embassy_rp::Peripherals {
        PIN_0,
        PIN_1,
        PIN_2,
        PIN_3,
        PIN_4,
        PIN_5,
        PIN_6,
        PIN_7,
        PIN_8,
        PIN_9,
        PIN_10,
        PIN_11,
        PIN_12,
        PIN_13,
        PIN_23,
        PIN_24,
        PIN_25,
        PIN_29,
        PIO0,
        DMA_CH0,
        FLASH,
        ..
    } = peripherals;

    let flash = FLASH_STORAGE.init(Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(
        FLASH,
    ));
    let stored_credentials = credential_store::load(&mut *flash)?;
    let stored_offset = load_timezone_offset(&mut *flash)?.unwrap_or(0);
    set_initial_utc_offset_minutes(stored_offset);

    let mut led = gpio::Output::new(PIN_0, Level::Low);
    led.set_low();

    let cells = OutputArray::new([
        gpio::Output::new(PIN_1, Level::High),
        gpio::Output::new(PIN_2, Level::High),
        gpio::Output::new(PIN_3, Level::High),
        gpio::Output::new(PIN_4, Level::High),
    ]);

    let segments = OutputArray::new([
        gpio::Output::new(PIN_5, Level::Low),
        gpio::Output::new(PIN_6, Level::Low),
        gpio::Output::new(PIN_7, Level::Low),
        gpio::Output::new(PIN_8, Level::Low),
        gpio::Output::new(PIN_9, Level::Low),
        gpio::Output::new(PIN_10, Level::Low),
        gpio::Output::new(PIN_11, Level::Low),
        gpio::Output::new(PIN_12, Level::Low),
    ]);

    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let mut clock = Clock::new(cells, segments, &CLOCK_NOTIFIER, spawner)?;
    let mut button = Button::new(gpio::Input::new(PIN_13, gpio::Pull::Down));

    let wifi_mode = if let Some(credentials) = stored_credentials {
        info!(
            "Stored Wi-Fi credentials found for SSID: {}",
            credentials.ssid
        );
        WifiMode::ClientConfigured(credentials)
    } else {
        info!("No stored Wi-Fi credentials - starting configuration access point");
        WifiMode::AccessPoint
    };

    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        PIN_23,
        PIN_25,
        PIO0,
        PIN_24,
        PIN_29,
        DMA_CH0,
        wifi_mode.clone(),
        spawner,
    );

    if matches!(wifi_mode, WifiMode::AccessPoint) {
        clock.show_access_point_setup().await;
        info!("Starting AP mode for credential capture");
        let stack = time_sync.wifi().stack().await;
        info!("Network stack ready in AP mode");

        let ap_ip = Ipv4Address::new(192, 168, 4, 1);
        let dns_token = unwrap!(dns_server_task(stack, ap_ip));
        spawner.spawn(dns_token);

        info!("Captive portal running - connect to PicoClockConfig and browse to http://192.168.4.1");
        let submission = collect_wifi_credentials(stack, spawner).await?;
        info!(
            "Credentials received for SSID: {} (offset {} minutes)",
            submission.credentials.ssid, submission.timezone_offset_minutes
        );

        credential_store::save(&mut *flash, &submission.credentials)?;
        save_timezone_offset(&mut *flash, submission.timezone_offset_minutes)?;
        set_initial_utc_offset_minutes(submission.timezone_offset_minutes);
        info!("Credentials saved; rebooting to apply client mode");
        Timer::after_millis(750).await;
        SCB::sys_reset();
    }

    info!("Wi-Fi ready; awaiting time synchronisation events");

    let mut persisted_offset = stored_offset;
    let mut state = if matches!(wifi_mode, WifiMode::AccessPoint) {
        ClockState::default()
    } else {
        ClockState::Connecting
    };

    loop {
        info!("State: {:?}", state);
        state = state.execute(&mut clock, &mut button, time_sync).await;

        if matches!(wifi_mode, WifiMode::ClientConfigured(_))
            && matches!(state, ClockState::AccessPointSetup)
        {
            info!("Connection timeout reached; clearing stored credentials and rebooting");
            credential_store::clear(&mut *flash)?;
            Timer::after_millis(500).await;
            info!("Resetting after automatic credential clear");
            SCB::sys_reset();
        }

        if let ClockState::ConfirmedClear = state {
            info!("Confirmed clear; erasing stored credentials and timezone offset");
            credential_store::clear(&mut *flash)?;
            clear_timezone_offset(&mut *flash)?;
            set_initial_utc_offset_minutes(0);
            clock.show_clearing_done().await;
            Timer::after_millis(750).await;
            info!("Resetting after flash clear");
            SCB::sys_reset();
        }

        let current_offset = current_utc_offset_minutes();
        if current_offset != persisted_offset {
            save_timezone_offset(&mut *flash, current_offset)?;
            persisted_offset = current_offset;
        }
    }
}
