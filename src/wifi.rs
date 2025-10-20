//! WiFi virtual device - manages WiFi connection and network stack

#![allow(clippy::future_not_send, reason = "single-threaded")]
#![allow(unsafe_code, reason = "StackStorage uses UnsafeCell in single-threaded context")]

use cyw43::JoinOptions;
use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::{Peri, bind_interrupts, peripherals};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use core::cell::UnsafeCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::waitqueue::AtomicWaker;
use embassy_time::Timer;
use portable_atomic::{AtomicBool, Ordering};
use static_cell::StaticCell;

// ============================================================================
// Types
// ============================================================================

/// Events emitted by the Wi-Fi device
pub enum WifiEvent {
    /// Network stack is initialized and DHCP is configured
    Ready,
    /// Link went down (future use)
    Down,
}

/// Single-threaded once-storage for network stack
/// 
/// SAFETY: This is safe in single-threaded Embassy context
pub struct StackStorage {
    initialized: AtomicBool,
    waker: AtomicWaker,
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
            waker: AtomicWaker::new(),
            value: UnsafeCell::new(None),
        }
    }
    
    /// Initialize the stack storage (can only be called once)
    pub fn init(&self, stack: &'static Stack<'static>) {
        if self.initialized.swap(true, Ordering::Release) {
            // Already initialized - this is a bug
            return;
        }
        
        // SAFETY: We just checked that we're the only initializer
        unsafe {
            *self.value.get() = Some(stack);
        }
        
        self.waker.wake();
    }
    
    /// Wait for the stack to be initialized and return it
    pub async fn get(&self) -> &'static Stack<'static> {
        core::future::poll_fn(|cx| {
            if self.initialized.load(Ordering::Acquire) {
                // SAFETY: initialized is true, so value is set
                let value = unsafe { (*self.value.get()).unwrap() };
                core::task::Poll::Ready(value)
            } else {
                self.waker.register(cx.waker());
                // Check again after registering to avoid race
                if self.initialized.load(Ordering::Acquire) {
                    let value = unsafe { (*self.value.get()).unwrap() };
                    core::task::Poll::Ready(value)
                } else {
                    core::task::Poll::Pending
                }
            }
        }).await
    }
}

// ============================================================================
// WiFi Virtual Device
// ============================================================================

pub type WifiNotifierInner = Signal<CriticalSectionRawMutex, WifiEvent>;

/// Resources needed by the WiFi device (single static)
pub struct WifiNotifier {
    pub notifier: WifiNotifierInner,
    pub stack_storage: StackStorage,
    wifi_cell: StaticCell<Wifi>,
}

/// WiFi virtual device - manages WiFi connection and emits network events
pub struct Wifi {
    notifier: &'static WifiNotifierInner,
    stack: &'static StackStorage,
}

impl Wifi {
    /// Create WiFi resources (notifier + storage)
    #[must_use]
    pub const fn notifier() -> WifiNotifier {
        WifiNotifier {
            notifier: Signal::new(),
            stack_storage: StackStorage::new(),
            wifi_cell: StaticCell::new(),
        }
    }
    
    /// Wait for the network stack to be ready and return a reference to it
    pub async fn wait_stack(&self) -> &'static Stack<'static> {
        self.stack.get().await
    }

    /// Create a new Wifi device and spawn its task
    /// Returns a static reference to the Wifi handle
    pub fn new(
        resources: &'static WifiNotifier,
        pin_23: Peri<'static, peripherals::PIN_23>,
        pin_25: Peri<'static, peripherals::PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, peripherals::PIN_24>,
        pin_29: Peri<'static, peripherals::PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        spawner: Spawner,
    ) -> &'static Self {
        unwrap!(spawner.spawn(wifi_device_loop(
            pin_23, pin_25, pio0, pin_24, pin_29, dma_ch0, &resources.notifier, &resources.stack_storage, spawner,
        )));
        resources.wifi_cell.init(Self { 
            notifier: &resources.notifier, 
            stack: &resources.stack_storage 
        })
    }

    /// Wait for and return the next WiFi event
    pub async fn next_event(&self) -> WifiEvent {
        self.notifier.wait().await
    }
}

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn wifi_device_loop(
    pin_23: Peri<'static, peripherals::PIN_23>,
    pin_25: Peri<'static, peripherals::PIN_25>,
    pio0: Peri<'static, PIO0>,
    pin_24: Peri<'static, peripherals::PIN_24>,
    pin_29: Peri<'static, peripherals::PIN_29>,
    dma_ch0: Peri<'static, DMA_CH0>,
    wifi_notifier: &'static WifiNotifierInner,
    stack_storage: &'static StackStorage,
    spawner: Spawner,
) -> ! {
    // Read WiFi credentials from compile-time environment
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");

    info!("WiFi device initializing");

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
    unwrap!(spawner.spawn(wifi_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // Initialize network stack
    let config = Config::dhcpv4(Default::default());
    let seed = 0x7c8f_3a2e_9d14_6b5a;

    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    static STACK: StaticCell<Stack<'static>> = StaticCell::new();
    let (stack_val, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<3>::new()),
        seed,
    );
    let stack = STACK.init(stack_val);

    unwrap!(spawner.spawn(net_task(runner)));

    // Connect to WiFi
    info!("Connecting to WiFi: {}", WIFI_SSID);
    loop {
        match control
            .join(WIFI_SSID, JoinOptions::new(WIFI_PASS.as_bytes()))
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

    info!("WiFi device ready");
    
    // Store stack reference and emit Ready event
    stack_storage.init(stack);
    wifi_notifier.signal(WifiEvent::Ready);

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
