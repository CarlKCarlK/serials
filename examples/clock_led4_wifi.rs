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
use defmt::{info, unwrap, warn};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_net::Ipv4Address;
use embassy_rp::gpio::{self, Level};
use embassy_time::Timer;
use serials::button::Button;
use serials::clock_led4::state::ClockLed4State;
use serials::clock_led4::{ClockLed4 as Clock, ClockLed4Notifier as ClockNotifier};
use serials::dns_server::dns_server_task;
use serials::flash_slice::{FlashArray, FlashArrayHandle, FlashBlock};
use serials::led4::OutputArray;
use serials::time_sync::{TimeSync, TimeSyncNotifier};
use serials::wifi::Wifi;
use serials::wifi_config::collect_wifi_credentials;
use serials::{Error, Result};
// Import clock_led4_time functions
use panic_probe as _;
use serials::clock_led4::time::{current_utc_offset_minutes, set_initial_utc_offset_minutes};

struct TimezoneStore {
    block: FlashBlock,
}

impl TimezoneStore {
    fn new(block: FlashBlock) -> Self {
        Self { block }
    }

    fn load(&mut self) -> Result<i32> {
        Ok(self.block.load::<i32>()?.unwrap_or(0))
    }

    fn save(&mut self, offset: i32) -> Result<()> {
        self.block.save(&offset)
    }

    fn clear(&mut self) -> Result<()> {
        self.block.clear()
    }
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting Wi-Fi 4-digit clock");
    let p = embassy_rp::init(Default::default());

    // Initialize flash storage
    static FLASH_HANDLE: FlashArrayHandle = FlashArray::<2>::handle();
    let [wifi_block, timezone_block] = FlashArray::new(&FLASH_HANDLE, p.FLASH)?;
    let mut timezone_store = TimezoneStore::new(timezone_block);
    let stored_offset = timezone_store.load()?;
    set_initial_utc_offset_minutes(stored_offset);
    let stored_offset = timezone_store.load()?;
    set_initial_utc_offset_minutes(stored_offset);

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
    let mut button = Button::new(p.PIN_13);

    // Determine initial boot phase by executing start logic
    // Initialize time sync
    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        p.PIN_23,  // WiFi chip data out
        p.PIN_25,  // WiFi chip data in
        p.PIO0,    // PIO for WiFi chip communication
        p.PIN_24,  // WiFi chip clock
        p.PIN_29,  // WiFi chip select
        p.DMA_CH0, // DMA channel for WiFi
        wifi_block,
        None,
        spawner,
    );

    let mut wifi_setup_state = WifiSetupState::execute_start(time_sync.wifi()).await?;

    // State machine loop
    loop {
        wifi_setup_state = wifi_setup_state
            .execute(
                &mut timezone_store,
                &mut clock,
                &mut button,
                &time_sync,
                spawner,
            )
            .await?;
    }
}

#[derive(Debug, Clone)]
enum WifiSetupState {
    CaptivePortal,
    AttemptConnection,
    Running,
}

impl WifiSetupState {
    /// Execute start-up logic to determine initial boot phase
    async fn execute_start(wifi: &Wifi) -> Result<Self> {
        Ok(if wifi.has_persisted_credentials() {
            info!("Stored credentials found - will attempt connection");
            Self::AttemptConnection
        } else {
            info!("No stored credentials - starting captive portal");
            Self::CaptivePortal
        })
    }

    async fn execute(
        self,
        timezone: &mut TimezoneStore,
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        spawner: Spawner,
    ) -> Result<Self> {
        match self {
            Self::CaptivePortal => {
                Self::execute_captive_portal(timezone, clock, time_sync, spawner).await
            }
            Self::AttemptConnection => {
                Self::execute_attempt_connection(clock, button, time_sync).await
            }
            Self::Running => Self::execute_running(clock, button, time_sync, timezone).await,
        }
    }

    async fn execute_captive_portal(
        timezone: &mut TimezoneStore,
        clock: &mut Clock<'_>,
        time_sync: &TimeSync,
        spawner: Spawner,
    ) -> Result<Self> {
        info!("WifiSetupState: CaptivePortal - starting captive portal");
        clock.show_access_point_setup().await;

        // Wait for AP to be fully initialized before getting stack
        time_sync.wifi().wait().await;
        let stack = time_sync.wifi().stack().await;
        info!("Network stack ready in AP mode");

        let ap_ip = Ipv4Address::new(192, 168, 4, 1);
        let dns_token = unwrap!(dns_server_task(stack, ap_ip));
        spawner.spawn(dns_token);

        info!("Captive portal running - connect to PicoClock and browse to http://192.168.4.1");
        let submission = collect_wifi_credentials(stack, spawner).await?;
        info!(
            "Credentials received for SSID: {} (offset {} minutes)",
            submission.credentials.ssid, submission.timezone_offset_minutes
        );

        time_sync
            .wifi()
            .persist_credentials(&submission.credentials)
            .map_err(|e| {
                warn!("{}", e);
                Error::StorageCorrupted
            })?;
        timezone.save(submission.timezone_offset_minutes)?;
        set_initial_utc_offset_minutes(submission.timezone_offset_minutes);

        info!("Credentials saved; rebooting to Start state");
        Timer::after_millis(750).await;
        SCB::sys_reset();
    }

    async fn execute_attempt_connection(
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Result<Self> {
        info!("WifiSetupState: AttemptConnection - attempting to connect and sync time");
        let mut clock_state = ClockLed4State::Connecting;

        // Keep trying to connect, checking for timeout
        loop {
            clock_state = clock_state.execute(clock, button, time_sync).await;

            // If we timeout, clear credentials and go back to AP mode
            if matches!(clock_state, ClockLed4State::AccessPointSetup) {
                info!("Connection timeout - clearing credentials and switching to AccessPoint");
                time_sync
                    .wifi()
                    .clear_persisted_credentials()
                    .map_err(|e| {
                        warn!("{}", e);
                        Error::StorageCorrupted
                    })?;
                Timer::after_millis(500).await;
                SCB::sys_reset();
            }

            // If we successfully synced time, move to ready state
            if matches!(clock_state, ClockLed4State::HoursMinutes) {
                info!("Time synced - moving to Running state");
                return Ok(Self::Running);
            }
        }
    }

    async fn execute_running(
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        timezone: &mut TimezoneStore,
    ) -> Result<Self> {
        info!("WifiSetupState: Running - clock operational");
        let mut clock_state = ClockLed4State::HoursMinutes;
        let mut persisted_offset = current_utc_offset_minutes();

        loop {
            clock_state = clock_state.execute(clock, button, time_sync).await;

            // Handle user confirming credential clear
            if let ClockLed4State::ConfirmedClear = clock_state {
                info!("Confirmed clear - erasing credentials and rebooting to Start");
                time_sync
                    .wifi()
                    .clear_persisted_credentials()
                    .map_err(|e| {
                        warn!("{}", e);
                        Error::StorageCorrupted
                    })?;
                timezone.clear()?;
                set_initial_utc_offset_minutes(0);
                clock.show_clearing_done().await;
                Timer::after_millis(750).await;
                SCB::sys_reset();
            }

            // Persist timezone offset changes
            let current_offset = current_utc_offset_minutes();
            if current_offset != persisted_offset {
                timezone.save(current_offset)?;
                persisted_offset = current_offset;
            }
        }
    }
}
