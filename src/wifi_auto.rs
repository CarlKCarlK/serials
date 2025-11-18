//! WiFi auto-provisioning with captive portal fallback.
//!
//! This module provides [`WifiAuto`], a high-level abstraction for managing WiFi
//! connections with automatic credential provisioning through a captive portal when
//! credentials are missing or connection fails.
//!
//! See [`WifiAuto`] for a complete example.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::{cell::RefCell, convert::Infallible, future::Future};
use cortex_m::peripheral::SCB;
use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_net::{Ipv4Address, Stack};
use embassy_rp::{
    Peri,
    gpio::Pin,
    peripherals::{DMA_CH0, PIN_23, PIN_24, PIN_25, PIN_29, PIO0},
};
use embassy_sync::{
    blocking_mutex::{Mutex, raw::CriticalSectionRawMutex},
    signal::Signal,
};
use embassy_time::{Duration, Timer, with_timeout};
use heapless::Vec;
use portable_atomic::{AtomicBool, Ordering};
use static_cell::StaticCell;

use crate::button::Button;
use crate::dns_server::dns_server_task;
use crate::flash_array::FlashBlock;
use crate::wifi::{Wifi, WifiEvent, WifiStartMode, WifiStatic};
use crate::wifi_config::WifiCredentials;
use crate::{Error, Result};

pub mod fields;
mod portal;

pub use portal::WifiAutoField;

/// Events emitted while provisioning or connecting.
#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum WifiAutoEvent {
    /// Captive portal is ready and waiting for user configuration.
    CaptivePortalReady,
    /// Attempting to connect to WiFi network.
    Connecting {
        /// Current attempt number (0-based).
        try_index: u8,
        /// Total number of attempts that will be made.
        try_count: u8,
    },
    /// Successfully connected to WiFi network.
    Connected,
}

const MAX_CONNECT_ATTEMPTS: u8 = 2;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(3);

pub type WifiAutoEvents = Signal<CriticalSectionRawMutex, WifiAutoEvent>;

const MAX_WIFI_AUTO_FIELDS: usize = 8;

/// Static for [`WifiAuto`]. See [`WifiAuto`] for usage example.
pub struct WifiAutoStatic {
    events: WifiAutoEvents,
    wifi: WifiStatic,
    wifi_auto_cell: StaticCell<WifiAuto>,
    force_captive_portal: AtomicBool,
    defaults: Mutex<CriticalSectionRawMutex, RefCell<Option<WifiCredentials>>>,
    button: Mutex<CriticalSectionRawMutex, RefCell<Option<Button<'static>>>>,
    fields_storage: StaticCell<Vec<&'static dyn WifiAutoField, MAX_WIFI_AUTO_FIELDS>>,
}

/// WiFi auto-provisioning with captive portal and custom configuration fields.
///
/// Manages WiFi connectivity with automatic fallback to a captive portal when credentials
/// are missing or invalid. Supports collecting additional configuration (e.g., timezone,
/// device name) through custom [`WifiAutoField`] implementations.
///
/// # Features
/// - Automatic captive portal on first boot or failed connections
/// - Customizable configuration fields beyond WiFi credentials
/// - Button-triggered reconfiguration
/// - Event-driven UI updates via [`connect`](Self::connect)
///
/// # Example
///
/// # Example
///
/// ```ignore
/// # use serials::flash_array::{FlashArray, FlashArrayStatic};
/// # use serials::wifi_auto::{WifiAuto, WifiAutoStatic, WifiAutoEvent};
/// # use serials::wifi_auto::fields::{TimezoneField, TimezoneFieldStatic};
/// # use embassy_executor::Spawner;
/// # use embassy_rp::peripherals;
/// # async fn example(
/// #     spawner: Spawner,
/// #     peripherals: peripherals::Peripherals,
/// # ) -> Result<(), serials::Error> {
/// // Set up flash storage for WiFi credentials and timezone
/// static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
/// let [wifi_flash, timezone_flash] =
///     FlashArray::new(&FLASH_STATIC, peripherals.FLASH)?;
///
/// // Create a timezone field to collect during provisioning
/// static TIMEZONE_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
/// let timezone_field = TimezoneField::new(&TIMEZONE_STATIC, timezone_flash);
///
/// // Initialize WifiAuto with the custom field
/// static WIFI_AUTO_STATIC: WifiAutoStatic = WifiAuto::new_static();
/// let wifi_auto = WifiAuto::new(
///     &WIFI_AUTO_STATIC,
///     peripherals.PIN_23,     // CYW43 power
///     peripherals.PIN_25,     // CYW43 chip select
///     peripherals.PIO0,       // CYW43 PIO interface
///     peripherals.PIN_24,     // CYW43 clock
///     peripherals.PIN_29,     // CYW43 data
///     peripherals.DMA_CH0,    // CYW43 DMA
///     wifi_flash,             // Flash for WiFi credentials
///     peripherals.PIN_13,     // Button for forced reconfiguration
///     "Pico",                 // Captive-portal SSID for provisioning
///     [timezone_field],       // Array of custom fields
///     spawner,
/// )?;
///
/// // Connect with UI feedback (blocks until connected)
/// // Note: If capturing variables from outer scope, create a reference first:
/// //   let display_ref = &display;
/// // Then use display_ref inside the closure.
/// let (stack, button) = wifi_auto
///     .connect(spawner, |event| async move {
///         match event {
///             WifiAutoEvent::CaptivePortalReady => {
///                 defmt::info!("Captive portal ready - connect to WiFi network");
///             }
///             WifiAutoEvent::Connecting { try_index, try_count } => {
///                 defmt::info!("Connecting to WiFi (attempt {} of {})...", try_index + 1, try_count);
///             }
///             WifiAutoEvent::Connected => {
///                 defmt::info!("WiFi connected successfully!");
///             }
///         }
///     })
///     .await?;
///
/// // Now connected - retrieve timezone configuration
/// let offset = timezone_field.offset_minutes()?.unwrap_or(0);
///
/// // Use stack for internet access and button for user interactions
/// // Example: fetch NTP time, make HTTP requests, etc.
/// # Ok(())
/// # }
/// ```
pub struct WifiAuto {
    events: &'static WifiAutoEvents,
    wifi: &'static Wifi,
    force_captive_portal: &'static AtomicBool,
    defaults: &'static Mutex<CriticalSectionRawMutex, RefCell<Option<WifiCredentials>>>,
    button: &'static Mutex<CriticalSectionRawMutex, RefCell<Option<Button<'static>>>>,
    fields: &'static [&'static dyn WifiAutoField],
}

impl WifiAutoStatic {
    #[must_use]
    pub const fn new() -> Self {
        WifiAutoStatic {
            events: Signal::new(),
            wifi: Wifi::new_static(),
            wifi_auto_cell: StaticCell::new(),
            force_captive_portal: AtomicBool::new(false),
            defaults: Mutex::new(RefCell::new(None)),
            button: Mutex::new(RefCell::new(None)),
            fields_storage: StaticCell::new(),
        }
    }

    fn force_captive_portal_flag(&'static self) -> &'static AtomicBool {
        &self.force_captive_portal
    }

    fn defaults(
        &'static self,
    ) -> &'static Mutex<CriticalSectionRawMutex, RefCell<Option<WifiCredentials>>> {
        &self.defaults
    }

    fn button(
        &'static self,
    ) -> &'static Mutex<CriticalSectionRawMutex, RefCell<Option<Button<'static>>>> {
        &self.button
    }
}

impl WifiAuto {
    /// Create static resources for [`WifiAuto`].
    ///
    /// See [`WifiAuto`] for a complete example.
    #[must_use]
    pub const fn new_static() -> WifiAutoStatic {
        WifiAutoStatic::new()
    }

    /// Initialize WiFi auto-provisioning with custom configuration fields.
    ///
    /// See [`WifiAuto`] for a complete example.
    #[allow(clippy::too_many_arguments)]
    pub fn new<const N: usize>(
        wifi_auto_static: &'static WifiAutoStatic,
        pin_23: Peri<'static, PIN_23>,
        pin_25: Peri<'static, PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, PIN_24>,
        pin_29: Peri<'static, PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        mut wifi_credentials_flash_block: FlashBlock,
        button_pin: Peri<'static, impl Pin>,
        captive_portal_ssid: &'static str,
        custom_fields: [&'static dyn WifiAutoField; N],
        spawner: Spawner,
    ) -> Result<&'static Self> {
        let stored_credentials = Wifi::peek_credentials(&mut wifi_credentials_flash_block);
        let stored_start_mode = Wifi::peek_start_mode(&mut wifi_credentials_flash_block);
        if matches!(stored_start_mode, WifiStartMode::CaptivePortal) {
            if let Some(creds) = stored_credentials.clone() {
                wifi_auto_static.defaults.lock(|cell| {
                    *cell.borrow_mut() = Some(creds);
                });
            }
        }

        let button = Button::new(button_pin);
        let force_captive_portal = button.is_pressed();
        if force_captive_portal {
            if let Some(creds) = stored_credentials.clone() {
                wifi_auto_static.defaults.lock(|cell| {
                    *cell.borrow_mut() = Some(creds);
                });
            }
            Wifi::prepare_start_mode(
                &mut wifi_credentials_flash_block,
                WifiStartMode::CaptivePortal,
            )
            .map_err(|_| Error::StorageCorrupted)?;
        }

        let wifi = Wifi::new_with_captive_portal_ssid(
            &wifi_auto_static.wifi,
            pin_23,
            pin_25,
            pio0,
            pin_24,
            pin_29,
            dma_ch0,
            wifi_credentials_flash_block,
            captive_portal_ssid,
            spawner,
        );

        wifi_auto_static.button.lock(|cell| {
            *cell.borrow_mut() = Some(button);
        });

        // Store fields array and convert to slice
        let fields_ref: &'static [&'static dyn WifiAutoField] = if N > 0 {
            assert!(
                N <= MAX_WIFI_AUTO_FIELDS,
                "WifiAuto supports at most {} custom fields",
                MAX_WIFI_AUTO_FIELDS
            );
            let mut storage: Vec<&'static dyn WifiAutoField, MAX_WIFI_AUTO_FIELDS> = Vec::new();
            for field in custom_fields {
                storage.push(field).unwrap_or_else(|_| unreachable!());
            }
            let stored_vec = wifi_auto_static.fields_storage.init(storage);
            stored_vec.as_slice()
        } else {
            &[]
        };

        let instance = wifi_auto_static.wifi_auto_cell.init(Self {
            events: &wifi_auto_static.events,
            wifi,
            force_captive_portal: wifi_auto_static.force_captive_portal_flag(),
            defaults: wifi_auto_static.defaults(),
            button: wifi_auto_static.button(),
            fields: fields_ref,
        });

        if force_captive_portal {
            instance.force_captive_portal();
        }

        Ok(instance)
    }

    fn force_captive_portal(&self) {
        self.force_captive_portal.store(true, Ordering::Relaxed);
    }

    /// Return the underlying [`Wifi`] handle for advanced operations such as clearing
    /// credentials. Avoid waiting on WiFi events while [`WifiAuto`] is running, as it
    /// already owns the event stream.
    pub fn wifi(&self) -> &'static Wifi {
        self.wifi
    }

    fn take_button(&self) -> Option<Button<'static>> {
        self.button.lock(|cell| cell.borrow_mut().take())
    }

    fn extra_fields_ready(&self) -> Result<bool> {
        for field in self.fields {
            if !field.is_satisfied().map_err(|_| Error::StorageCorrupted)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn wait_event(&self) -> WifiAutoEvent {
        self.events.wait().await
    }

    /// Ensures WiFi connection with UI callback for event-driven status updates.
    ///
    /// Automatically monitors connection events and awaits the provided callback for
    /// each event. The callback can be either synchronous (no `.await` calls) or
    /// asynchronous (with `.await` calls for async operations like updating displays).
    ///
    /// The future resolves once WiFi connectivity is established and returns access to
    /// the network stack plus the reconfiguration button.
    ///
    /// # Examples
    ///
    /// Synchronous callback (no `.await` calls):
    /// ```ignore
    /// wifi_auto.connect(spawner, |event| async move {
    ///     info!("Event: {:?}", event);
    /// }).await?;
    /// ```
    ///
    /// Asynchronous callback (with `.await` calls):
    /// ```ignore
    /// wifi_auto.connect(spawner, |event| async move {
    ///     display.set_state(event_to_state(event)).await;
    /// }).await?;
    /// ```
    pub async fn connect<Fut, F>(
        &self,
        spawner: Spawner,
        mut on_event: F,
    ) -> Result<(&'static Stack<'static>, Button<'static>)>
    where
        F: FnMut(WifiAutoEvent) -> Fut,
        Fut: Future<Output = ()>,
    {
        let ui = async {
            loop {
                let event = self.wait_event().await;
                on_event(event).await;

                if matches!(event, WifiAutoEvent::Connected) {
                    break;
                }
            }
        };

        let (result, ()) = embassy_futures::join::join(self.ensure_connected(spawner), ui).await;
        result?;
        let stack = self.wifi.stack().await;
        let button = self.take_button().ok_or(Error::StorageCorrupted)?;
        Ok((stack, button))
    }

    async fn ensure_connected(&self, spawner: Spawner) -> Result<()> {
        loop {
            let force_captive_portal = self.force_captive_portal.swap(false, Ordering::AcqRel);
            let start_mode = self.wifi.current_start_mode();
            let has_creds = self.wifi.has_persisted_credentials();
            let extras_ready = self.extra_fields_ready()?;
            if force_captive_portal
                || matches!(start_mode, WifiStartMode::CaptivePortal)
                || !has_creds
                || !extras_ready
            {
                if has_creds {
                    if let Some(creds) = self.wifi.load_persisted_credentials() {
                        self.defaults.lock(|cell| {
                            *cell.borrow_mut() = Some(creds);
                        });
                    }
                }
                self.events.signal(WifiAutoEvent::CaptivePortalReady);
                self.run_captive_portal(spawner).await?;
                unreachable!("Device should reset after captive portal submission");
            }

            for attempt in 1..=MAX_CONNECT_ATTEMPTS {
                info!(
                    "WifiAuto: connection attempt {}/{}",
                    attempt, MAX_CONNECT_ATTEMPTS
                );
                self.events.signal(WifiAutoEvent::Connecting {
                    try_index: attempt - 1,
                    try_count: MAX_CONNECT_ATTEMPTS,
                });
                if self
                    .wait_for_client_ready_with_timeout(CONNECT_TIMEOUT)
                    .await
                {
                    self.events.signal(WifiAutoEvent::Connected);
                    return Ok(());
                }
                warn!("WifiAuto: connection attempt {} timed out", attempt);
                Timer::after(RETRY_DELAY).await;
            }

            info!(
                "WifiAuto: failed to connect after {} attempts, returning to captive portal",
                MAX_CONNECT_ATTEMPTS
            );
            if let Some(creds) = self.wifi.load_persisted_credentials() {
                self.defaults.lock(|cell| {
                    *cell.borrow_mut() = Some(creds);
                });
            }
            self.wifi
                .set_start_mode(WifiStartMode::CaptivePortal)
                .map_err(|_| Error::StorageCorrupted)?;
            Timer::after_millis(500).await;
            SCB::sys_reset();
        }
    }

    async fn wait_for_client_ready_with_timeout(&self, timeout: Duration) -> bool {
        with_timeout(timeout, async {
            loop {
                match self.wifi.wait().await {
                    WifiEvent::ClientReady => break,
                    WifiEvent::CaptivePortalReady => {
                        info!(
                            "WifiAuto: received captive-portal-ready event while waiting for client mode"
                        );
                    }
                }
            }
        })
        .await
        .is_ok()
    }

    #[allow(unreachable_code)]
    async fn run_captive_portal(&self, spawner: Spawner) -> Result<Infallible> {
        self.wifi.wait().await;
        let stack = self.wifi.stack().await;

        let captive_portal_ip = Ipv4Address::new(192, 168, 4, 1);
        let dns_token = unwrap!(dns_server_task(stack, captive_portal_ip));
        spawner.spawn(dns_token);

        let defaults_owned = self
            .defaults
            .lock(|cell| cell.borrow_mut().take())
            .or_else(|| self.wifi.load_persisted_credentials());
        let submission =
            portal::collect_credentials(stack, spawner, defaults_owned.as_ref(), self.fields)
                .await?;
        self.wifi.persist_credentials(&submission).map_err(|err| {
            warn!("{}", err);
            Error::StorageCorrupted
        })?;

        Timer::after_millis(750).await;
        SCB::sys_reset();
        loop {
            cortex_m::asm::nop();
        }
    }
}
