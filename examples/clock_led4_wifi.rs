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
use embassy_rp::gpio::{self, Level};
use embassy_time::Timer;
use serials::Result;
use serials::button::Button;
use serials::clock_led4::state::ClockLed4State;
use serials::clock_led4::{ClockLed4 as Clock, ClockLed4Notifier as ClockNotifier};
use serials::dns_server::dns_server_task;
use serials::flash::{Flash, FlashNotifier};
use serials::led4::OutputArray;
use serials::time_sync::{TimeSync, TimeSyncNotifier};
use serials::wifi_config::{WifiCredentials, collect_wifi_credentials};
// Import clock_led4_time functions
use panic_probe as _;
use serials::clock_led4::time::{current_utc_offset_minutes, set_initial_utc_offset_minutes};

struct WifiFlashStore {
    credentials: Flash,
    timezone: Flash,
}

impl WifiFlashStore {
    fn new(credentials: Flash, timezone: Flash) -> Self {
        Self {
            credentials,
            timezone,
        }
    }

    fn load_credentials(&mut self) -> Result<Option<WifiCredentials>> {
        self.credentials.load(0)
    }

    fn save_credentials(&mut self, creds: &WifiCredentials) -> Result<()> {
        self.credentials.save(0, creds)
    }

    fn clear_credentials(&mut self) -> Result<()> {
        self.credentials.clear(0)
    }

    fn load_timezone_offset(&mut self) -> Result<i32> {
        Ok(self.timezone.load::<i32>(0)?.unwrap_or(0))
    }

    fn save_timezone_offset(&mut self, offset: &i32) -> Result<()> {
        self.timezone.save(0, offset)
    }

    fn clear_timezone_offset(&mut self) -> Result<()> {
        self.timezone.clear(0)
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
    static FLASH_NOTIFIER: FlashNotifier = Flash::notifier();
    let flash_root = Flash::new(&FLASH_NOTIFIER, p.FLASH);
    let (credentials_flash, rest) = flash_root.take_first();
    let (timezone_flash, _) = rest.take_first();
    let mut flash = WifiFlashStore::new(credentials_flash, timezone_flash);

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
    let mut wifi_setup_state = WifiSetupState::execute_start(&mut flash).await?;

    // Initialize time sync with WiFi credentials from initial state
    // NOTE: In the future, a typestate pattern could enforce at compile-time that
    // execute_start never returns Running, eliminating the runtime check.
    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        p.PIN_23,  // WiFi chip data out
        p.PIN_25,  // WiFi chip data in
        p.PIO0,    // PIO for WiFi chip communication
        p.PIN_24,  // WiFi chip clock
        p.PIN_29,  // WiFi chip select
        p.DMA_CH0, // DMA channel for WiFi
        wifi_setup_state.credentials_if_any(),
        spawner,
    );

    // State machine loop
    loop {
        wifi_setup_state = wifi_setup_state
            .execute(&mut flash, &mut clock, &mut button, &time_sync, spawner)
            .await?;
    }
}

#[derive(Debug, Clone)]
enum WifiSetupState {
    CaptivePortal,
    AttemptConnection(WifiCredentials),
    Running,
}

impl WifiSetupState {
    /// Execute start-up logic to determine initial boot phase
    async fn execute_start(flash: &mut WifiFlashStore) -> Result<Self> {
        info!("Loading credentials from flash");
        let stored_credentials: Option<WifiCredentials> = flash.load_credentials()?;
        let stored_offset: i32 = flash.load_timezone_offset()?;
        set_initial_utc_offset_minutes(stored_offset);

        Ok(if let Some(credentials) = stored_credentials {
            info!("Stored credentials found - will attempt connection");
            Self::AttemptConnection(credentials)
        } else {
            info!("No stored credentials - starting captive portal");
            Self::CaptivePortal
        })
    }

    /// Extract WiFi credentials from boot phase
    ///
    /// Returns None for AccessPoint mode, Some(credentials) for client mode.
    /// Panics if called on Running state (should never happen from execute_start,
    /// but a typestate pattern could enforce this at compile-time in the future).
    fn credentials_if_any(&self) -> Option<WifiCredentials> {
        match self {
            Self::CaptivePortal => None,
            Self::AttemptConnection(credentials) => Some(credentials.clone()),
            Self::Running => {
                // This should never happen if execute_start is implemented correctly.
                // A typestate pattern could prevent this at compile-time.
                panic!("Invalid state: execute_start should never return Running")
            }
        }
    }

    async fn execute(
        self,
        flash: &mut WifiFlashStore,
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        spawner: Spawner,
    ) -> Result<Self> {
        match self {
            Self::CaptivePortal => {
                Self::execute_captive_portal(flash, clock, time_sync, spawner).await
            }
            Self::AttemptConnection(_credentials) => {
                Self::execute_attempt_connection(clock, button, time_sync, flash).await
            }
            Self::Running => Self::execute_running(clock, button, time_sync, flash).await,
        }
    }

    async fn execute_captive_portal(
        flash: &mut WifiFlashStore,
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

        flash.save_credentials(&submission.credentials)?;
        flash.save_timezone_offset(&submission.timezone_offset_minutes)?;
        set_initial_utc_offset_minutes(submission.timezone_offset_minutes);

        info!("Credentials saved; rebooting to Start state");
        Timer::after_millis(750).await;
        SCB::sys_reset();
    }

    async fn execute_attempt_connection(
        clock: &mut Clock<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        flash: &mut WifiFlashStore,
    ) -> Result<Self> {
        info!("WifiSetupState: AttemptConnection - attempting to connect and sync time");
        let mut clock_state = ClockLed4State::Connecting;

        // Keep trying to connect, checking for timeout
        loop {
            clock_state = clock_state.execute(clock, button, time_sync).await;

            // If we timeout, clear credentials and go back to AP mode
            if matches!(clock_state, ClockLed4State::AccessPointSetup) {
                info!("Connection timeout - clearing credentials and switching to AccessPoint");
                flash.clear_credentials()?;
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
        flash: &mut WifiFlashStore,
    ) -> Result<Self> {
        info!("WifiSetupState: Running - clock operational");
        let mut clock_state = ClockLed4State::HoursMinutes;
        let mut persisted_offset = current_utc_offset_minutes();

        loop {
            clock_state = clock_state.execute(clock, button, time_sync).await;

            // Handle user confirming credential clear
            if let ClockLed4State::ConfirmedClear = clock_state {
                info!("Confirmed clear - erasing credentials and rebooting to Start");
                flash.clear_credentials()?;
                flash.clear_timezone_offset()?;
                set_initial_utc_offset_minutes(0);
                clock.show_clearing_done().await;
                Timer::after_millis(750).await;
                SCB::sys_reset();
            }

            // Persist timezone offset changes
            let current_offset = current_utc_offset_minutes();
            if current_offset != persisted_offset {
                flash.save_timezone_offset(&current_offset)?;
                persisted_offset = current_offset;
            }
        }
    }
}
