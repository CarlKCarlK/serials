//! TimeSync virtual device - manages NTP synchronization over WiFi

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use cyw43::JoinOptions;
use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::{Config, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;

use crate::unix_seconds::UnixSeconds;
use crate::Result;

// ============================================================================
// Types
// ============================================================================

#[derive(Clone)]
pub enum TimeSyncEvent {
    SyncSuccess { unix_seconds: UnixSeconds },
    SyncFailed(&'static str),
}

// ============================================================================
// TimeSync Virtual Device
// ============================================================================

pub type TimeSyncNotifier = Signal<CriticalSectionRawMutex, TimeSyncEvent>;

/// TimeSync virtual device - manages NTP synchronization
pub struct TimeSync(&'static TimeSyncNotifier);

impl TimeSync {
    /// Create the notifier for TimeSync
    #[must_use]
    pub const fn notifier() -> TimeSyncNotifier {
        Signal::new()
    }

    /// Create a new TimeSync device and spawn its task
    pub fn new(
        pin_23: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_23>,
        pin_25: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_25>,
        pio0: embassy_rp::Peri<'static, PIO0>,
        pin_24: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_24>,
        pin_29: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_29>,
        dma_ch0: embassy_rp::Peri<'static, DMA_CH0>,
        notifier: &'static TimeSyncNotifier,
        spawner: Spawner,
    ) -> Self {
        unwrap!(spawner.spawn(time_sync_device_loop(
            pin_23, pin_25, pio0, pin_24, pin_29, dma_ch0, notifier, spawner,
        )));
        Self(notifier)
    }

    /// Wait for and return the next time sync event
    pub async fn next_event(&self) -> TimeSyncEvent {
        self.0.wait().await
    }
}

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn time_sync_device_loop(
    pin_23: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_23>,
    pin_25: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_25>,
    pio0: embassy_rp::Peri<'static, PIO0>,
    pin_24: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_24>,
    pin_29: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_29>,
    dma_ch0: embassy_rp::Peri<'static, DMA_CH0>,
    sync_notifier: &'static TimeSyncNotifier,
    spawner: Spawner,
) -> ! {
    let err = inner_time_sync_device_loop(
        pin_23,
        pin_25,
        pio0,
        pin_24,
        pin_29,
        dma_ch0,
        sync_notifier,
        spawner,
    )
    .await
    .unwrap_err();
    core::panic!("{err}");
}

async fn inner_time_sync_device_loop(
    pin_23: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_23>,
    pin_25: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_25>,
    pio0: embassy_rp::Peri<'static, PIO0>,
    pin_24: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_24>,
    pin_29: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_29>,
    dma_ch0: embassy_rp::Peri<'static, DMA_CH0>,
    sync_notifier: &'static TimeSyncNotifier,
    spawner: Spawner,
) -> Result<Infallible> {
    // Read WiFi credentials from compile-time environment
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");

    info!("TimeSync device started");

    // Initialize WiFi and network stack
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

    let config = Config::dhcpv4(Default::default());
    let seed = 0x7c8f_3a2e_9d14_6b5a;

    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    static STACK: StaticCell<embassy_net::Stack<'static>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<3>::new()),
        seed,
    );
    let stack = STACK.init(stack);

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

    // Initial sync with retry (exponential backoff: 10s, 30s, 60s, then 5min intervals)
    let mut attempt = 0;
    loop {
        attempt += 1;
        info!("Sync attempt {}", attempt);
        match fetch_ntp_time(stack).await {
            Ok(unix_seconds) => {
                info!("Initial sync successful: unix_seconds={}", unix_seconds.as_i64());

                sync_notifier.signal(TimeSyncEvent::SyncSuccess { unix_seconds });
                break;
            }
            Err(e) => {
                info!("Sync failed: {}", e);
                sync_notifier.signal(TimeSyncEvent::SyncFailed(e));
                // Exponential backoff: 10s, 30s, 60s, then 5min intervals
                let delay_secs = if attempt == 1 {
                    10
                } else if attempt == 2 {
                    30
                } else if attempt == 3 {
                    60
                } else {
                    300 // 5 minutes for subsequent attempts
                };
                info!("Sync failed, retrying in {}s...", delay_secs);
                Timer::after_secs(delay_secs).await;
            }
        }
    }

    // Hourly sync loop (on failure, retry every 5 minutes)
    let mut last_success_elapsed = 0_u64;
    loop {
        // Wait 1 hour after last success, or 5 minutes after failure
        let wait_secs = if last_success_elapsed == 0 { 3600 } else { 300 };
        Timer::after_secs(wait_secs).await;
        last_success_elapsed = last_success_elapsed.saturating_add(wait_secs);

        info!(
            "Periodic sync ({}s since last success)...",
            last_success_elapsed
        );
        match fetch_ntp_time(stack).await {
            Ok(unix_seconds) => {
                info!("Periodic sync successful: unix_seconds={}", unix_seconds.as_i64());

                sync_notifier.signal(TimeSyncEvent::SyncSuccess { unix_seconds });
                last_success_elapsed = 0; // Reset counter on success
            }
            Err(e) => {
                info!("Sync failed: {}", e);
                sync_notifier.signal(TimeSyncEvent::SyncFailed(e));
                info!("Sync failed, will retry in 5 minutes");
            }
        }
    }
}

// ============================================================================
// Network - NTP Fetch
// ============================================================================

async fn fetch_ntp_time(stack: &embassy_net::Stack<'static>) -> Result<UnixSeconds, &'static str> {
    use embassy_net::dns::DnsQueryType;
    use embassy_net::udp::UdpSocket;

    // NTP server
    const NTP_SERVER: &str = "pool.ntp.org";
    const NTP_PORT: u16 = 123;

    // DNS lookup
    info!("Resolving {}...", NTP_SERVER);
    let dns_result = stack
        .dns_query(NTP_SERVER, DnsQueryType::A)
        .await
        .map_err(|e| {
            warn!("DNS lookup failed: {:?}", e);
            "DNS lookup failed"
        })?;
    let server_addr = dns_result.first().ok_or("No DNS results")?;

    info!("NTP Server IP: {}", server_addr);

    // Create UDP socket
    let mut rx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 1];
    let mut rx_buffer = [0; 128];
    let mut tx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 1];
    let mut tx_buffer = [0; 128];
    let mut socket = UdpSocket::new(
        *stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    socket.bind(0).map_err(|e| {
        warn!("Socket bind failed: {:?}", e);
        "Socket bind failed"
    })?;

    // Build NTP request (48 bytes, version 3, client mode)
    let mut ntp_request = [0u8; 48];
    ntp_request[0] = 0x1B; // LI=0, VN=3, Mode=3 (client)

    // Send request
    info!("Sending NTP request to {}...", server_addr);
    socket
        .send_to(&ntp_request, (*server_addr, NTP_PORT))
        .await
        .map_err(|e| {
            warn!("NTP send failed: {:?}", e);
            "NTP send failed"
        })?;

    // Receive response with timeout
    let mut response = [0u8; 48];
    let (n, _from) =
        embassy_time::with_timeout(Duration::from_secs(5), socket.recv_from(&mut response))
            .await
            .map_err(|_| {
                warn!("NTP receive timeout");
                "NTP receive timeout"
            })?
            .map_err(|e| {
                warn!("NTP receive failed: {:?}", e);
                "NTP receive failed"
            })?;

    if n < 48 {
        warn!("NTP response too short: {} bytes", n);
        return Err("NTP response too short");
    }

    // Extract transmit timestamp (bytes 40-47, big-endian)
    let ntp_seconds = u32::from_be_bytes([response[40], response[41], response[42], response[43]]);

    // Convert NTP timestamp to Unix seconds
    let unix_time = UnixSeconds::from_ntp_seconds(ntp_seconds)
        .ok_or("Invalid NTP timestamp")?;

    info!("NTP time: {} (unix timestamp)", unix_time.as_i64());
    Ok(unix_time)
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
