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
    let p = embassy_rp::init(Default::default());
    let flash = FLASH_STORAGE.init(Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(
        p.FLASH,
    ));

    // Initialize LED (unused but kept for compatibility)
    let mut led = gpio::Output::new(p.PIN_0, Level::Low);
    led.set_low();

    // Initialize display pins
    let cells = OutputArray::new([
        gpio::Output::new(p.PIN_1, Level::High),
        gpio::Output::new(p.PIN_2, Level::High),
        gpio::Output::new(p.PIN_3, Level::High),
        gpio::Output::new(p.PIN_4, Level::High),
    ]);

    let segments = OutputArray::new([
        gpio::Output::new(p.PIN_5, Level::Low),
        gpio::Output::new(p.PIN_6, Level::Low),
        gpio::Output::new(p.PIN_7, Level::Low),
        gpio::Output::new(p.PIN_8, Level::Low),
        gpio::Output::new(p.PIN_9, Level::Low),
        gpio::Output::new(p.PIN_10, Level::Low),
        gpio::Output::new(p.PIN_11, Level::Low),
        gpio::Output::new(p.PIN_12, Level::Low),
    ]);

    // Initialize clock and button
    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let mut clock = Clock::new(cells, segments, &CLOCK_NOTIFIER, spawner)?;
    let mut button = Button::new(gpio::Input::new(p.PIN_13, gpio::Pull::Down));

    // Determine initial WiFi mode based on stored credentials
    let stored_credentials = credential_store::load(&mut *flash)?;
    let wifi_mode = if let Some(credentials) = stored_credentials {
        info!("Stored Wi-Fi credentials found for SSID: {}", credentials.ssid);
        WifiMode::ClientConfigured(credentials)
    } else {
        info!("No stored Wi-Fi credentials - starting configuration access point");
        WifiMode::AccessPoint
    };

    // Initialize time sync
    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        p.PIN_23,
        p.PIN_25,
        p.PIO0,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
        wifi_mode,
        spawner,
    );

    // State machine loop
    let mut state = WifiSetupState::Start;
    loop {
        state = state.execute(flash, &mut clock, &mut button, &time_sync, spawner).await?;
    }
}


#[derive(Debug, defmt::Format, Clone, Copy)]
enum WifiSetupState {
    Start,
    AccessPoint,
    TryConnect,
    ReadyToWork,
}

impl WifiSetupState {
    async fn execute(
        self,
        flash: &mut Flash<'static, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        spawner: Spawner,
    ) -> Result<Self> {
        match self {
            Self::Start => Self::execute_start(flash).await,
            Self::AccessPoint => Self::execute_access_point(flash, clock, time_sync, spawner).await,
            Self::TryConnect => Self::execute_try_connect(clock, button, time_sync, flash).await,
            Self::ReadyToWork => Self::execute_ready_to_work(clock, button, time_sync, flash).await,
        }
    }

    async fn execute_start(
        flash: &mut Flash<'static, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    ) -> Result<Self> {
        info!("State: Start - loading credentials");
        let stored_credentials = credential_store::load(flash)?;
        let stored_offset = load_timezone_offset(flash)?.unwrap_or(0);
        set_initial_utc_offset_minutes(stored_offset);

        Ok(if stored_credentials.is_some() {
            Self::TryConnect
        } else {
            Self::AccessPoint
        })
    }

    async fn execute_access_point(
        flash: &mut Flash<'static, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
        clock: &mut Clock<'_>,
        time_sync: &TimeSync,
        spawner: Spawner,
    ) -> Result<Self> {
        info!("State: AccessPoint - starting captive portal");
        clock.show_access_point_setup().await;
        
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

        credential_store::save(flash, &submission.credentials)?;
        save_timezone_offset(flash, submission.timezone_offset_minutes)?;
        set_initial_utc_offset_minutes(submission.timezone_offset_minutes);
        
        info!("Credentials saved; rebooting to Start state");
        Timer::after_millis(750).await;
        SCB::sys_reset();
    }

    async fn execute_try_connect(
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        flash: &mut Flash<'static, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    ) -> Result<Self> {
        info!("State: TryConnect - attempting to connect and sync time");
        let mut clock_state = ClockState::Connecting;

        // Keep trying to connect, checking for timeout
        loop {
            clock_state = clock_state.execute(clock, button, time_sync).await;
            
            // If we timeout, clear credentials and go back to AP mode
            if matches!(clock_state, ClockState::AccessPointSetup) {
                info!("Connection timeout - clearing credentials and switching to AccessPoint");
                credential_store::clear(flash)?;
                Timer::after_millis(500).await;
                SCB::sys_reset();
            }
            
            // If we successfully synced time, move to ready state
            if matches!(clock_state, ClockState::HoursMinutes) {
                info!("Time synced - moving to ReadyToWork state");
                return Ok(Self::ReadyToWork);
            }
        }
    }

    async fn execute_ready_to_work(
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        flash: &mut Flash<'static, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    ) -> Result<Self> {
        info!("State: ReadyToWork - running clock");
        let mut clock_state = ClockState::HoursMinutes;
        let mut persisted_offset = current_utc_offset_minutes();

        loop {
            clock_state = clock_state.execute(clock, button, time_sync).await;

            // Handle user confirming credential clear
            if let ClockState::ConfirmedClear = clock_state {
                info!("Confirmed clear - erasing credentials and rebooting to Start");
                credential_store::clear(flash)?;
                clear_timezone_offset(flash)?;
                set_initial_utc_offset_minutes(0);
                clock.show_clearing_done().await;
                Timer::after_millis(750).await;
                SCB::sys_reset();
            }

            // Persist timezone offset changes
            let current_offset = current_utc_offset_minutes();
            if current_offset != persisted_offset {
                save_timezone_offset(flash, current_offset)?;
                persisted_offset = current_offset;
            }
        }
    }
}
