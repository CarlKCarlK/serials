//! WiFi auto-provisioning helper that falls back to a captive portal when
//! credentials are missing.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::{cell::RefCell, convert::Infallible};
use cortex_m::peripheral::SCB;
use defmt::{info, warn, unwrap};
use embassy_executor::Spawner;
use embassy_net::Ipv4Address;
use embassy_rp::{Peri, peripherals::{DMA_CH0, PIN_23, PIN_24, PIN_25, PIN_29, PIO0}};
use embassy_sync::{blocking_mutex::{raw::CriticalSectionRawMutex, Mutex}, signal::Signal};
use embassy_time::Timer;
use portable_atomic::{AtomicBool, Ordering};
use static_cell::StaticCell;

use crate::button::Button;
use crate::dns_server::dns_server_task;
use crate::flash_array::FlashBlock;
use crate::wifi::{Wifi, WifiEvent, WifiNotifier};
use crate::wifi_config::{collect_wifi_credentials, WifiConfigOptions, WifiCredentials};
use crate::{Error, Result};

/// Events emitted while provisioning or connecting.
#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum WifiAutoEvent {
    CaptivePortalReady,
    ClientConnecting,
    Connected,
}

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
}

impl WifiAutoNotifier {
    fn force_ap_flag(&'static self) -> &'static AtomicBool {
        &self.force_ap
    }

    fn defaults(&'static self) -> &'static Mutex<CriticalSectionRawMutex, RefCell<Option<WifiCredentials>>> {
        &self.defaults
    }

    fn button(&'static self) -> &'static Mutex<CriticalSectionRawMutex, RefCell<Option<Button<'static>>>> {
        &self.button
    }
}

impl WifiAuto {
    #[must_use]
    pub const fn notifier() -> WifiAutoNotifier {
        WifiAutoNotifier {
            events: Signal::new(),
            wifi: Wifi::notifier(),
            wifi_auto_cell: StaticCell::new(),
            force_ap: AtomicBool::new(false),
            defaults: Mutex::new(RefCell::new(None)),
            button: Mutex::new(RefCell::new(None)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        resources: &'static WifiAutoNotifier,
        pin_23: Peri<'static, PIN_23>,
        pin_25: Peri<'static, PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, PIN_24>,
        pin_29: Peri<'static, PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        mut credential_store: FlashBlock,
        button_pin: Peri<'static, impl embassy_rp::gpio::Pin>,
        ap_ssid: &'static str,
        spawner: Spawner,
    ) -> Result<&'static Self> {
        let stored_credentials = credential_store.load::<WifiCredentials>()?;
        let button = Button::new(button_pin);
        let force_ap = button.is_pressed();
        if force_ap {
            if let Some(creds) = stored_credentials.clone() {
                resources.defaults.lock(|cell| {
                    *cell.borrow_mut() = Some(creds);
                });
            }
            credential_store.clear()?;
        }

        let wifi = Wifi::new(
            &resources.wifi,
            pin_23,
            pin_25,
            pio0,
            pin_24,
            pin_29,
            dma_ch0,
            credential_store,
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

    pub async fn wait_event(&self) -> WifiAutoEvent {
        self.events.wait().await
    }

    pub async fn ensure_connected(&self, spawner: Spawner) -> Result<()> {
        let force_portal = self.force_ap.swap(false, Ordering::AcqRel);
        if force_portal || !self.wifi.has_persisted_credentials() {
            self.events.signal(WifiAutoEvent::CaptivePortalReady);
            self.run_captive_portal(spawner).await?;
            unreachable!("Device should reset after captive portal submission");
        }

        self.events.signal(WifiAutoEvent::ClientConnecting);
        self.wait_for_client_ready().await;
        self.events.signal(WifiAutoEvent::Connected);
        Ok(())
    }

    async fn wait_for_client_ready(&self) {
        loop {
            match self.wifi.wait().await {
                WifiEvent::ClientReady => break,
                WifiEvent::ApReady => {
                    info!("WifiAuto: received AP-ready event while waiting for client mode");
                }
            }
        }
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
        let options = WifiConfigOptions::with_defaults(defaults_owned.as_ref());
        let submission = collect_wifi_credentials(stack, spawner, options).await?;
        self.wifi
            .persist_credentials(&submission.credentials)
            .map_err(|err| {
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
