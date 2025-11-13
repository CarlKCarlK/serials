//! WiFi device abstraction supporting both access point and client modes.
//!
//! This module provides a high-level interface for managing WiFi connectivity on the
//! Raspberry Pi Pico W. It supports two main operating modes:
//!
//! - **Access Point (AP) mode**: Creates a WiFi hotspot for device configuration
//! - **Client mode**: Connects to an existing WiFi network
//!
//! # Examples
//!
//! ## Provisioning via captive portal
//!
//! ```no_run
//! # #![no_std]
//! # #![no_main]
//! # use panic_probe as _;
//! # use core::default::Default;
//! use serials::flash_slice::{FlashArray, FlashArrayHandle};
//! use serials::wifi::Wifi;
//!
//! # async fn example(spawner: embassy_executor::Spawner) {
//! let p = embassy_rp::init(Default::default());
//!
//! static WIFI_NOTIFIER: serials::wifi::WifiNotifier = Wifi::notifier();
//! static FLASH_HANDLE: FlashArrayHandle = FlashArray::<1>::handle();
//! let [wifi_block] = FlashArray::new(&FLASH_HANDLE, p.FLASH).unwrap();
//!
//! // Start in AP mode for user configuration
//! let wifi = Wifi::new(
//!     &WIFI_NOTIFIER,
//!     p.PIN_23,
//!     p.PIN_25,
//!     p.PIO0,
//!     p.PIN_24,
//!     p.PIN_29,
//!     p.DMA_CH0,
//!     wifi_block,
//!     None,
//!     spawner,
//! );
//!
//! // Wait for AP to be ready
//! wifi.wait().await;
//!
//! // Get network stack for serving configuration interface
//! let stack = wifi.stack().await;
//! // ... serve web interface on 192.168.4.1 ...
//! # }
//! ```
//!
//! ## Client mode with stored credentials
//!
//! ```no_run
//! # #![no_std]
//! # #![no_main]
//! # use panic_probe as _;
//! # use core::default::Default;
//! use serials::flash_slice::{FlashArray, FlashArrayHandle};
//! use serials::wifi::Wifi;
//! use serials::wifi_config::WifiCredentials;
//!
//! # async fn example(spawner: embassy_executor::Spawner, credentials: WifiCredentials) {
//! let p = embassy_rp::init(Default::default());
//!
//! static WIFI_NOTIFIER: serials::wifi::WifiNotifier = Wifi::notifier();
//! static FLASH_HANDLE: FlashArrayHandle = FlashArray::<1>::handle();
//! let [wifi_block] = FlashArray::new(&FLASH_HANDLE, p.FLASH).unwrap();
//!
//! // Connect using credentials that were provisioned earlier (e.g., loaded from flash)
//! let wifi = Wifi::new(
//!     &WIFI_NOTIFIER,
//!     p.PIN_23,
//!     p.PIN_25,
//!     p.PIO0,
//!     p.PIN_24,
//!     p.PIN_29,
//!     p.DMA_CH0,
//!     wifi_block,
//!     Some(credentials),
//!     spawner,
//! );
//!
//! wifi.wait().await;
//! let stack = wifi.stack().await;
//! // ... use stack ...
//! # }
//! ```

#![allow(clippy::future_not_send, reason = "single-threaded")]
#![allow(
    unsafe_code,
    reason = "StackStorage uses UnsafeCell in single-threaded context"
)]

use core::cell::{RefCell, UnsafeCell};
use cyw43::JoinOptions;
use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_24, PIN_25, PIN_29, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::{Peri, bind_interrupts};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use portable_atomic::{AtomicBool, Ordering};
use static_cell::StaticCell;

use crate::dhcp_server::dhcp_server_task;
use crate::flash_slice::FlashBlock;
use crate::wifi_config::WifiCredentials;

// ============================================================================
// Types
// ============================================================================

/// Events emitted by the WiFi device.
pub enum WifiEvent {
    /// Network stack is initialized in AP mode, ready for configuration
    ApReady,
    /// Network stack is initialized in client mode and DHCP is configured
    ClientReady,
}

/// Internal WiFi operating mode used during startup.
#[derive(Clone, PartialEq, Eq)]
enum WifiMode {
    /// Start in AP mode for configuration (no credentials needed)
    AccessPoint,
    /// Connect to WiFi network using provisioned credentials
    ClientConfigured(WifiCredentials),
}

/// Single-threaded once-storage for network stack
///
/// SAFETY: This is safe in single-threaded Embassy context
pub struct StackStorage {
    initialized: AtomicBool,
    ready: Signal<CriticalSectionRawMutex, ()>,
    value: UnsafeCell<Option<&'static Stack<'static>>>,
}

// SAFETY: We're in a single-threaded context (Embassy on Pico)
unsafe impl Sync for StackStorage {}

impl StackStorage {
    /// Create a new empty StackStorage
    #[must_use]
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            ready: Signal::new(),
            value: UnsafeCell::new(None),
        }
    }

    /// Initialize the stack storage (can only be called once)
    pub fn init(&self, stack: &'static Stack<'static>) {
        // SAFETY: This is called once from WiFi device loop
        unsafe {
            *self.value.get() = Some(stack);
        }
        self.initialized.store(true, Ordering::Release);
        self.ready.signal(());
    }

    /// Wait for the stack to be initialized and return it
    pub async fn get(&self) -> &'static Stack<'static> {
        if !self.initialized.load(Ordering::Acquire) {
            self.ready.wait().await;
        }
        // SAFETY: initialized is true, so value is set
        unsafe { (*self.value.get()).unwrap() }
    }
}

// ============================================================================
// WiFi Virtual Device
// ============================================================================

/// Signal type for WiFi events.
pub type WifiEvents = Signal<CriticalSectionRawMutex, WifiEvent>;

/// Resources needed by the WiFi device.
pub struct WifiNotifier {
    events: WifiEvents,
    stack: StackStorage,
    wifi_cell: StaticCell<Wifi>,
}

/// A device abstraction that manages WiFi connectivity and network stack in both AP and client modes.
///
/// See the [module-level documentation](crate::wifi) for usage examples.
pub struct Wifi {
    events: &'static WifiEvents,
    stack: &'static StackStorage,
    credential_store: Mutex<CriticalSectionRawMutex, RefCell<FlashBlock>>,
}

impl Wifi {
    /// Create WiFi resources (notifier + storage).
    ///
    /// This must be called once to create a static `WifiNotifier` that will be passed to [`Wifi::new`].
    ///
    /// See the [module-level documentation](crate::wifi) for usage examples.
    #[must_use]
    pub const fn notifier() -> WifiNotifier {
        WifiNotifier {
            events: Signal::new(),
            stack: StackStorage::new(),
            wifi_cell: StaticCell::new(),
        }
    }

    /// Wait for the network stack to be ready and return a reference to it.
    ///
    /// This provides access to the Embassy network stack for TCP/UDP operations.
    /// The stack will be configured differently depending on the WiFi mode:
    /// - In AP mode: static IP 192.168.4.1
    /// - In client mode: DHCP-assigned IP
    ///
    /// See the [module-level documentation](crate::wifi) for usage examples.
    pub async fn stack(&self) -> &'static Stack<'static> {
        self.stack.get().await
    }

    /// Wait for and return the next WiFi event.
    ///
    /// This will block until the next [`WifiEvent`] occurs, such as:
    /// - [`WifiEvent::ApReady`] when access point mode is initialized
    /// - [`WifiEvent::ClientReady`] when connected to WiFi and DHCP is configured
    ///
    /// See the [module-level documentation](crate::wifi) for usage examples.
    pub async fn wait(&self) -> WifiEvent {
        self.events.wait().await
    }

    /// Create a new WiFi device and spawn its background task.
    ///
    /// This initializes the WiFi hardware and spawns tasks to manage the WiFi connection
    /// and network stack. Returns a static reference to the WiFi handle.
    ///
    /// # Arguments
    ///
    /// * `resources` - Static WiFi resources created with [`Wifi::notifier`]
    /// * `pin_23` - WiFi chip power pin (GPIO 23)
    /// * `pin_25` - WiFi chip chip select pin (GPIO 25)
    /// * `pio0` - PIO peripheral for WiFi communication
    /// * `pin_24` - WiFi chip clock pin (GPIO 24)
    /// * `pin_29` - WiFi chip data pin (GPIO 29)
    /// * `dma_ch0` - DMA channel for WiFi SPI communication
    /// * `credential_store` - Flash block reserved for WiFi credentials
    /// * `credentials` - `Some` to start in client mode immediately, `None` to check persisted creds / launch AP
    /// * `spawner` - Embassy task spawner
    ///
    /// See the [module-level documentation](crate::wifi) for usage examples.
    pub fn new(
        resources: &'static WifiNotifier,
        pin_23: Peri<'static, PIN_23>,
        pin_25: Peri<'static, PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, PIN_24>,
        pin_29: Peri<'static, PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        credential_store: FlashBlock,
        credentials: Option<WifiCredentials>,
        spawner: Spawner,
    ) -> &'static Self {
        let mut store_block = credential_store;
        let stored_credentials = match store_block.load::<WifiCredentials>() {
            Ok(value) => value,
            Err(_e) => {
                warn!(
                    "Failed to load stored WiFi credentials (block {})",
                    store_block.block_id()
                );
                None
            }
        };

        let mode = if let Some(creds) = credentials {
            WifiMode::ClientConfigured(creds)
        } else if let Some(creds) = stored_credentials {
            WifiMode::ClientConfigured(creds)
        } else {
            WifiMode::AccessPoint
        };
        let token = unwrap!(wifi_device_loop(
            pin_23,
            pin_25,
            pio0,
            pin_24,
            pin_29,
            dma_ch0,
            mode,
            &resources.events,
            &resources.stack,
            spawner,
        ));
        spawner.spawn(token);
        resources.wifi_cell.init(Self {
            events: &resources.events,
            stack: &resources.stack,
            credential_store: Mutex::new(RefCell::new(store_block)),
        })
    }

    /// Reconfigure WiFi to client mode with provided credentials
    /// This is called after collecting credentials in AP mode
    pub async fn switch_to_client_mode(
        &self,
        credentials: WifiCredentials,
    ) -> Result<(), &'static str> {
        info!("Switching to client mode with SSID: {}", credentials.ssid);
        // For now, we'll need to restart the device to switch modes
        // This is a limitation - full implementation would need control handle
        Err("Mode switch requires device restart - not yet implemented")
    }

    fn with_credential_store<F>(&self, f: F) -> Result<(), &'static str>
    where
        F: FnOnce(&mut FlashBlock) -> Result<(), &'static str>,
    {
        self.credential_store.lock(|cell| {
            let mut block = cell.borrow_mut();
            f(&mut *block)
        })
    }

    /// Persist credentials into the configured flash store.
    pub fn persist_credentials(&self, credentials: &WifiCredentials) -> Result<(), &'static str> {
        self.with_credential_store(|block| {
            block
                .save(credentials)
                .map_err(|_| "Failed to save WiFi credentials")
        })
    }

    /// Remove any stored credentials from flash.
    pub fn clear_persisted_credentials(&self) -> Result<(), &'static str> {
        self.with_credential_store(|block| {
            block
                .clear()
                .map_err(|_| "Failed to clear WiFi credentials")
        })
    }

    /// Return whether credentials currently exist in flash.
    pub fn has_persisted_credentials(&self) -> bool {
        self.credential_store.lock(|cell| {
            let mut block = cell.borrow_mut();
            match block.load::<WifiCredentials>() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(_) => {
                    warn!(
                        "Failed to load stored WiFi credentials (block {})",
                        block.block_id()
                    );
                    false
                }
            }
        })
    }
}

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn wifi_device_loop(
    pin_23: Peri<'static, PIN_23>,
    pin_25: Peri<'static, PIN_25>,
    pio0: Peri<'static, PIO0>,
    pin_24: Peri<'static, PIN_24>,
    pin_29: Peri<'static, PIN_29>,
    dma_ch0: Peri<'static, DMA_CH0>,
    mode: WifiMode,
    wifi_events: &'static WifiEvents,
    stack_storage: &'static StackStorage,
    spawner: Spawner,
) -> ! {
    match mode {
        WifiMode::AccessPoint => {
            wifi_device_loop_ap(
                pin_23,
                pin_25,
                pio0,
                pin_24,
                pin_29,
                dma_ch0,
                wifi_events,
                stack_storage,
                spawner,
            )
            .await
        }
        WifiMode::ClientConfigured(credentials) => {
            wifi_device_loop_client_configured(
                pin_23,
                pin_25,
                pio0,
                pin_24,
                pin_29,
                dma_ch0,
                wifi_events,
                stack_storage,
                spawner,
                credentials,
            )
            .await
        }
    }
}

/// WiFi device loop for AP mode
async fn wifi_device_loop_ap(
    pin_23: Peri<'static, PIN_23>,
    pin_25: Peri<'static, PIN_25>,
    pio0: Peri<'static, PIO0>,
    pin_24: Peri<'static, PIN_24>,
    pin_29: Peri<'static, PIN_29>,
    dma_ch0: Peri<'static, DMA_CH0>,
    wifi_events: &'static WifiEvents,
    stack_storage: &'static StackStorage,
    spawner: Spawner,
) -> ! {
    info!("WiFi device initializing in AP mode");

    // Initialize WiFi hardware
    let fw = cyw43_firmware::CYW43_43439A0;
    let clm = cyw43_firmware::CYW43_43439A0_CLM;

    let pwr = Output::new(pin_23, Level::Low);
    let cs = Output::new(pin_25, Level::High);
    let mut pio = Pio::new(pio0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        pin_24,
        pin_29,
        dma_ch0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    let wifi_token = unwrap!(wifi_task(runner));
    spawner.spawn(wifi_token);

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // Start AP mode
    const AP_SSID: &str = "PicoClock";
    const AP_PASSWORD: &str = ""; // Open network

    info!("Starting AP mode: {}", AP_SSID);

    // Configure static IP for AP mode (we are the gateway)
    let config = Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: embassy_net::Ipv4Cidr::new(embassy_net::Ipv4Address::new(192, 168, 4, 1), 24),
        gateway: Some(embassy_net::Ipv4Address::new(192, 168, 4, 1)),
        dns_servers: heapless::Vec::from_slice(&[embassy_net::Ipv4Address::new(192, 168, 4, 1)])
            .unwrap(),
    });

    let seed = 0x0bad_cafe_dead_beef;

    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    static STACK: StaticCell<Stack<'static>> = StaticCell::new();
    let (stack_val, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<5>::new()),
        seed,
    );
    let stack = STACK.init(stack_val);

    let net_token = unwrap!(net_task(runner));
    spawner.spawn(net_token);

    // Start AP
    if AP_PASSWORD.is_empty() {
        control.start_ap_open(AP_SSID, 1).await;
    } else {
        control.start_ap_wpa2(AP_SSID, AP_PASSWORD, 1).await;
    }

    info!("AP mode started! SSID: {}", AP_SSID);

    stack.wait_config_up().await;

    if let Some(config) = stack.config_v4() {
        info!("AP IP Address: {}", config.address);
    }

    // Start DHCP server for AP mode
    let server_ip = embassy_net::Ipv4Address::new(192, 168, 4, 1);
    let netmask = embassy_net::Ipv4Address::new(255, 255, 255, 0);
    let pool_start = embassy_net::Ipv4Address::new(192, 168, 4, 2);
    let pool_size = 253; // 192.168.4.2 - 192.168.4.254

    let dhcp_token = unwrap!(dhcp_server_task(
        stack, server_ip, netmask, pool_start, pool_size,
    ));
    spawner.spawn(dhcp_token);

    info!("DHCP server started (pool: 192.168.4.2-254)");
    info!("WiFi AP ready - connect to '{}'", AP_SSID);

    // Store stack reference and emit ApReady event
    stack_storage.init(stack);
    wifi_events.signal(WifiEvent::ApReady);

    // Keep task alive
    loop {
        Timer::after_secs(3600).await;
    }
}

/// WiFi device loop for client mode with provisioned credentials
async fn wifi_device_loop_client_configured(
    pin_23: Peri<'static, PIN_23>,
    pin_25: Peri<'static, PIN_25>,
    pio0: Peri<'static, PIO0>,
    pin_24: Peri<'static, PIN_24>,
    pin_29: Peri<'static, PIN_29>,
    dma_ch0: Peri<'static, DMA_CH0>,
    wifi_events: &'static WifiEvents,
    stack_storage: &'static StackStorage,
    spawner: Spawner,
    credentials: WifiCredentials,
) -> ! {
    let WifiCredentials { ssid, password } = credentials;

    wifi_device_loop_client_impl(
        pin_23,
        pin_25,
        pio0,
        pin_24,
        pin_29,
        dma_ch0,
        wifi_events,
        stack_storage,
        spawner,
        ssid,
        password,
    )
    .await
}

/// Shared client-mode implementation.
async fn wifi_device_loop_client_impl(
    pin_23: Peri<'static, PIN_23>,
    pin_25: Peri<'static, PIN_25>,
    pio0: Peri<'static, PIO0>,
    pin_24: Peri<'static, PIN_24>,
    pin_29: Peri<'static, PIN_29>,
    dma_ch0: Peri<'static, DMA_CH0>,
    wifi_events: &'static WifiEvents,
    stack_storage: &'static StackStorage,
    spawner: Spawner,
    ssid: heapless::String<32>,
    password: heapless::String<64>,
) -> ! {
    info!("WiFi device initializing in client mode");

    let ssid_str = ssid;
    let password_str = password;

    // Initialize WiFi hardware
    let fw = cyw43_firmware::CYW43_43439A0;
    let clm = cyw43_firmware::CYW43_43439A0_CLM;

    let pwr = Output::new(pin_23, Level::Low);
    let cs = Output::new(pin_25, Level::High);
    let mut pio = Pio::new(pio0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        pin_24,
        pin_29,
        dma_ch0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    let wifi_token = unwrap!(wifi_task(runner));
    spawner.spawn(wifi_token);

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // Initialize network stack
    let config = Config::dhcpv4(Default::default());
    let seed = 0x7c8f_3a2e_9d14_6b5a;

    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    static STACK: StaticCell<Stack<'static>> = StaticCell::new();
    let (stack_val, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<5>::new()),
        seed,
    );
    let stack = STACK.init(stack_val);

    let net_token = unwrap!(net_task(runner));
    spawner.spawn(net_token);

    // Connect to WiFi
    info!("Connecting to WiFi: {}", ssid_str);
    loop {
        match control
            .join(ssid_str.as_str(), JoinOptions::new(password_str.as_bytes()))
            .await
        {
            Ok(_) => break,
            Err(err) => {
                info!("Join failed: {}", err.status);
                Timer::after_secs(1).await;
            }
        }
    }

    info!("WiFi connected! Waiting for DHCP...");
    stack.wait_config_up().await;

    if let Some(config) = stack.config_v4() {
        info!("IP Address: {}", config.address);
    }

    info!("WiFi client ready");

    // Store stack reference and emit ClientReady event
    stack_storage.init(stack);
    wifi_events.signal(WifiEvent::ClientReady);

    // Keep task alive (could monitor link status in future)
    loop {
        Timer::after_secs(3600).await;
    }
}

// ============================================================================
// WiFi Tasks
// ============================================================================

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}
