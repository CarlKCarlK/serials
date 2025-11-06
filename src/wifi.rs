//! WiFi virtual device - manages WiFi connection and network stack
//!
//! Supports two modes:
//! 1. AP Mode: Start as access point for configuration
//! 2. Client Mode: Connect to existing WiFi network

#![allow(clippy::future_not_send, reason = "single-threaded")]
#![allow(
    unsafe_code,
    reason = "StackStorage uses UnsafeCell in single-threaded context"
)]

use core::cell::UnsafeCell;
use cyw43::JoinOptions;
use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_24, PIN_25, PIN_29, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::{Peri, bind_interrupts};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use portable_atomic::{AtomicBool, Ordering};
use static_cell::StaticCell;

use crate::dhcp_server::dhcp_server_task;
use crate::wifi_config::WifiCredentials;

// ============================================================================
// Types
// ============================================================================

/// Events emitted by the Wi-Fi device
pub enum WifiEvent {
    /// Network stack is initialized in AP mode, ready for configuration
    ApReady,
    /// Network stack is initialized in client mode and DHCP is configured
    ClientReady,
}

/// WiFi operating mode
#[derive(Clone, PartialEq, Eq)]
pub enum WifiMode {
    /// Start in AP mode for configuration (no credentials needed)
    AccessPoint,
    /// Connect to existing WiFi network with compile-time credentials
    ClientStatic,
    /// Connect to WiFi network using runtime-provided credentials
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

pub type WifiEvents = Signal<CriticalSectionRawMutex, WifiEvent>;

/// Resources needed by the WiFi device (single static)
pub struct WifiNotifier {
    events: WifiEvents,
    stack: StackStorage,
    wifi_cell: StaticCell<Wifi>,
}

/// WiFi virtual device - manages WiFi connection and emits network events
pub struct Wifi {
    events: &'static WifiEvents,
    stack: &'static StackStorage,
}

impl Wifi {
    /// Create WiFi resources (notifier + storage)
    #[must_use]
    pub const fn notifier() -> WifiNotifier {
        WifiNotifier {
            events: Signal::new(),
            stack: StackStorage::new(),
            wifi_cell: StaticCell::new(),
        }
    }

    /// Wait for the network stack to be ready and return a reference to it
    pub async fn stack(&self) -> &'static Stack<'static> {
        self.stack.get().await
    }

    /// Wait for and return the next WiFi event
    pub async fn wait(&self) -> WifiEvent {
        self.events.wait().await
    }

    /// Create a new Wifi device and spawn its task
    /// Returns a static reference to the Wifi handle
    ///
    /// # Arguments
    /// * `mode` - WiFi mode (AccessPoint or Client)
    pub fn new(
        resources: &'static WifiNotifier,
        pin_23: Peri<'static, PIN_23>,
        pin_25: Peri<'static, PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, PIN_24>,
        pin_29: Peri<'static, PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        mode: WifiMode,
        spawner: Spawner,
    ) -> &'static Self {
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
        WifiMode::ClientStatic => {
            wifi_device_loop_client_static(
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
    const AP_SSID: &str = "PicoConfig";
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

/// WiFi device loop for client mode with compile-time credentials
async fn wifi_device_loop_client_static(
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
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");

    let mut ssid = heapless::String::<32>::new();
    let mut password = heapless::String::<64>::new();
    unwrap!(ssid.push_str(WIFI_SSID));
    unwrap!(password.push_str(WIFI_PASS));

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

/// WiFi device loop for client mode with runtime credentials
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
