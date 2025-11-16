//! WiFi auto-provisioning helper that falls back to a captive portal when
//! credentials are missing.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::{cell::RefCell, convert::Infallible};
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
use portable_atomic::{AtomicBool, Ordering};
use static_cell::StaticCell;

use crate::button::Button;
use crate::dns_server::dns_server_task;
use crate::flash_array::FlashBlock;
use crate::wifi::{Wifi, WifiEvent, WifiNotifier, WifiStartMode};
use crate::wifi_auto_portal::{self};
use crate::wifi_config::WifiCredentials;
use crate::{Error, Result};

pub use crate::wifi_auto_portal::{FormData, HtmlBuffer, WifiAutoField};

/// Events emitted while provisioning or connecting.
#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum WifiAutoEvent {
    CaptivePortalReady,
    ClientConnecting { try_index: u8, try_count: u8 },
    Connected,
}

pub struct WifiAutoConfig {
    pub fields: &'static [&'static dyn WifiAutoField],
}

impl WifiAutoConfig {
    #[must_use]
    pub const fn new() -> Self {
        Self { fields: &[] }
    }

    #[must_use]
    pub const fn with_fields(mut self, fields: &'static [&'static dyn WifiAutoField]) -> Self {
        self.fields = fields;
        self
    }
}

impl Default for WifiAutoConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WifiAutoConnected {
    pub stack: &'static Stack<'static>,
    pub button: Button<'static>,
}

pub struct WifiAutoHandle {
    inner: &'static WifiAuto,
}

const MAX_CONNECT_ATTEMPTS: u8 = 2;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(3);

pub type WifiAutoEvents = Signal<CriticalSectionRawMutex, WifiAutoEvent>;

pub struct WifiAutoNotifier {
    events: WifiAutoEvents,
    wifi: WifiNotifier,
    wifi_auto_cell: StaticCell<WifiAuto>,
    force_ap: AtomicBool,
    defaults: Mutex<CriticalSectionRawMutex, RefCell<Option<WifiCredentials>>>,
    button: Mutex<CriticalSectionRawMutex, RefCell<Option<Button<'static>>>>,
}

pub struct WifiAuto {
    events: &'static WifiAutoEvents,
    wifi: &'static Wifi,
    force_ap: &'static AtomicBool,
    defaults: &'static Mutex<CriticalSectionRawMutex, RefCell<Option<WifiCredentials>>>,
    button: &'static Mutex<CriticalSectionRawMutex, RefCell<Option<Button<'static>>>>,
    ap_ssid: &'static str,
    fields: &'static [&'static dyn WifiAutoField],
}

impl WifiAutoNotifier {
    #[must_use]
    pub const fn new() -> Self {
        WifiAutoNotifier {
            events: Signal::new(),
            wifi: Wifi::notifier(),
            wifi_auto_cell: StaticCell::new(),
            force_ap: AtomicBool::new(false),
            defaults: Mutex::new(RefCell::new(None)),
            button: Mutex::new(RefCell::new(None)),
        }
    }

    fn force_ap_flag(&'static self) -> &'static AtomicBool {
        &self.force_ap
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
    #[must_use]
    pub const fn notifier() -> WifiAutoNotifier {
        WifiAutoNotifier::new()
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        resources: &'static WifiAutoNotifier,
        pin_23: Peri<'static, PIN_23>,
        pin_25: Peri<'static, PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, PIN_24>,
        pin_29: Peri<'static, PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        mut credential_store: FlashBlock,
        button_pin: Peri<'static, impl Pin>,
        ap_ssid: &'static str,
        config: WifiAutoConfig,
        spawner: Spawner,
    ) -> Result<&'static Self> {
        let WifiAutoConfig { fields } = config;

        let stored_credentials = Wifi::peek_credentials(&mut credential_store);
        let stored_start_mode = Wifi::peek_start_mode(&mut credential_store);
        if matches!(stored_start_mode, WifiStartMode::AccessPoint) {
            if let Some(creds) = stored_credentials.clone() {
                resources.defaults.lock(|cell| {
                    *cell.borrow_mut() = Some(creds);
                });
            }
        }

        let button = Button::new(button_pin);
        let force_ap = button.is_pressed();
        if force_ap {
            if let Some(creds) = stored_credentials.clone() {
                resources.defaults.lock(|cell| {
                    *cell.borrow_mut() = Some(creds);
                });
            }
            Wifi::prepare_start_mode(&mut credential_store, WifiStartMode::AccessPoint)
                .map_err(|_| Error::StorageCorrupted)?;
        }

        let wifi = Wifi::new_with_ap_ssid(
            &resources.wifi,
            pin_23,
            pin_25,
            pio0,
            pin_24,
            pin_29,
            dma_ch0,
            credential_store,
            ap_ssid,
            spawner,
        );

        resources.button.lock(|cell| {
            *cell.borrow_mut() = Some(button);
        });

        let instance = resources.wifi_auto_cell.init(Self {
            events: &resources.events,
            wifi,
            force_ap: resources.force_ap_flag(),
            defaults: resources.defaults(),
            button: resources.button(),
            ap_ssid,
            fields,
        });

        if force_ap {
            instance.force_captive_portal();
        }

        Ok(instance)
    }

    pub fn wifi(&self) -> &'static Wifi {
        self.wifi
    }

    pub fn ap_ssid(&self) -> &'static str {
        self.ap_ssid
    }

    pub fn force_captive_portal(&self) {
        self.force_ap.store(true, Ordering::Relaxed);
    }

    pub fn set_default_credentials(&self, credentials: WifiCredentials) {
        self.defaults.lock(|cell| {
            *cell.borrow_mut() = Some(credentials);
        });
    }

    pub fn take_button(&self) -> Option<Button<'static>> {
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

    pub async fn wait_event(&self) -> WifiAutoEvent {
        self.events.wait().await
    }

    /// Ensures WiFi connection with UI callback for event-driven status updates.
    ///
    /// This is a convenience wrapper around [`ensure_connected`](Self::ensure_connected)
    /// that automatically monitors connection events and invokes a callback for each event,
    /// eliminating the need for manual `join()` and event loop boilerplate.
    ///
    /// # Parameters
    /// - `spawner`: Embassy task spawner for background WiFi tasks
    /// - `on_event`: Callback invoked for each [`WifiAutoEvent`] during connection
    ///
    /// # Returns
    /// - `Ok(())` when successfully connected to WiFi
    /// - `Err(_)` if flash operations fail or other unrecoverable errors occur
    ///
    /// # Example
    /// ```no_run
    /// let WifiAutoConnected { stack, button, .. } =
    ///     wifi_auto.ensure_connected_with_ui(spawner, |event| match event {
    ///         WifiAutoEvent::CaptivePortalReady => {
    ///             led4.write_text(BlinkState::BlinkingAndOn, ['C', 'O', 'N', 'N']);
    ///         }
    ///         WifiAutoEvent::ClientConnecting { try_index, .. } => {
    ///             led4.animate_text(animation(try_index));
    ///         }
    ///         WifiAutoEvent::Connected => {
    ///             led4.write_text(BlinkState::Solid, ['D', 'O', 'N', 'E']);
    ///         }
    ///     }).await?;
    /// ```
    pub async fn ensure_connected_with_ui<F>(
        &self,
        spawner: Spawner,
        mut on_event: F,
    ) -> Result<WifiAutoConnected>
    where
        F: FnMut(WifiAutoEvent),
    {
        let ui = async {
            loop {
                let event = self.wait_event().await;
                on_event(event);

                if matches!(event, WifiAutoEvent::Connected) {
                    break;
                }
            }
        };

        let (result, ()) = embassy_futures::join::join(self.ensure_connected(spawner), ui).await;
        result?;
        let stack = self.wifi.stack().await;
        let button = self.take_button().ok_or(Error::StorageCorrupted)?;
        Ok(WifiAutoConnected { stack, button })
    }

    async fn ensure_connected(&self, spawner: Spawner) -> Result<()> {
        loop {
            let force_portal = self.force_ap.swap(false, Ordering::AcqRel);
            let start_mode = self.wifi.current_start_mode();
            let has_creds = self.wifi.has_persisted_credentials();
            let extras_ready = self.extra_fields_ready()?;
            if force_portal
                || matches!(start_mode, WifiStartMode::AccessPoint)
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
                self.events.signal(WifiAutoEvent::ClientConnecting {
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
                .set_start_mode(WifiStartMode::AccessPoint)
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
                    WifiEvent::ApReady => {
                        info!("WifiAuto: received AP-ready event while waiting for client mode");
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

        let ap_ip = Ipv4Address::new(192, 168, 4, 1);
        let dns_token = unwrap!(dns_server_task(stack, ap_ip));
        spawner.spawn(dns_token);

        let defaults_owned = self
            .defaults
            .lock(|cell| cell.borrow_mut().take())
            .or_else(|| self.wifi.load_persisted_credentials());
        let submission = wifi_auto_portal::collect_credentials(
            stack,
            spawner,
            defaults_owned.as_ref(),
            self.fields,
        )
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

impl WifiAutoHandle {
    #[must_use]
    pub const fn notifier() -> WifiAutoNotifier {
        WifiAutoNotifier::new()
    }

    pub fn new(
        resources: &'static WifiAutoNotifier,
        pin_23: Peri<'static, PIN_23>,
        pin_25: Peri<'static, PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, PIN_24>,
        pin_29: Peri<'static, PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        credential_store: FlashBlock,
        button_pin: Peri<'static, impl Pin>,
        ap_ssid: &'static str,
        config: WifiAutoConfig,
        spawner: Spawner,
    ) -> Result<Self> {
        WifiAuto::new(
            resources,
            pin_23,
            pin_25,
            pio0,
            pin_24,
            pin_29,
            dma_ch0,
            credential_store,
            button_pin,
            ap_ssid,
            config,
            spawner,
        )
        .map(|inner| Self { inner })
    }

    pub async fn ensure_connected_with_ui<F>(
        self,
        spawner: Spawner,
        on_event: F,
    ) -> Result<WifiAutoConnected>
    where
        F: FnMut(WifiAutoEvent),
    {
        self.inner.ensure_connected_with_ui(spawner, on_event).await
    }
}
