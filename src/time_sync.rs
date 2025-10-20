//! TimeSync virtual device - manages NTP synchronization

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::peripherals::{PIN_23, PIN_24, PIN_25, PIN_29, PIO0, DMA_CH0};
use embassy_net::{Stack, dns, udp};
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;

use crate::unix_seconds::UnixSeconds;
use crate::wifi::{Wifi, WifiNotifier};
use crate::Result;

// ============================================================================
// Types
// ============================================================================

#[derive(Clone)]
pub enum TimeSyncEvent {
    SyncSuccess { unix_seconds: UnixSeconds },
    SyncFailed(&'static str),
}

pub type TimeSyncEvents = Signal<CriticalSectionRawMutex, TimeSyncEvent>;

/// Resources needed by TimeSync device (includes WiFi resources)
pub struct TimeSyncNotifier {
    events: TimeSyncEvents,
    wifi: WifiNotifier,
    time_sync_cell: StaticCell<TimeSync>,
}

// ============================================================================
// TimeSync Virtual Device
// ============================================================================

/// TimeSync virtual device - manages time synchronization
pub struct TimeSync {
    events: &'static TimeSyncEvents,
    #[allow(dead_code, reason = "Keeps WiFi alive")]
    wifi: &'static Wifi,
}

impl TimeSync {
    /// Create TimeSync resources (includes WiFi)
    #[must_use]
    pub const fn notifier() -> TimeSyncNotifier {
        TimeSyncNotifier {
            events: Signal::new(),
            wifi: Wifi::notifier(),
            time_sync_cell: StaticCell::new(),
        }
    }

    /// Create a new TimeSync device (creates WiFi internally) and spawn its task
    pub fn new(
        resources: &'static TimeSyncNotifier,
        pin_23: Peri<'static, PIN_23>,
        pin_25: Peri<'static, PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, PIN_24>,
        pin_29: Peri<'static, PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        spawner: Spawner,
    ) -> &'static Self {
        // Create WiFi device
        let wifi = Wifi::new(
            &resources.wifi,
            pin_23,
            pin_25,
            pio0,
            pin_24,
            pin_29,
            dma_ch0,
            spawner,
        );

        // Spawn TimeSync task
        unwrap!(spawner.spawn(time_sync_device_loop(wifi, &resources.events)));
        
        resources.time_sync_cell.init(Self {
            events: &resources.events,
            wifi,
        })
    }

    /// Wait for and return the next TimeSync event
    pub async fn wait(&self) -> TimeSyncEvent {
        self.events.wait().await
    }
}

#[embassy_executor::task]
async fn time_sync_device_loop(
    wifi: &'static Wifi,
    sync_events: &'static TimeSyncEvents,
) -> ! {
    let err = inner_time_sync_device_loop(wifi, sync_events)
        .await
        .unwrap_err();
    core::panic!("{err}");
}

async fn inner_time_sync_device_loop(
    wifi: &'static Wifi,
    sync_events: &'static TimeSyncEvents,
) -> Result<Infallible> {
    info!("TimeSync device awaiting network stack...");
    
    // Wait for WiFi to be ready and get the stack
    let stack = wifi.wait_stack().await;
    info!("TimeSync received network stack");
    
    info!("TimeSync device started");

    // Initial sync with retry (exponential backoff: 10s, 30s, 60s, then 5min intervals)
    let mut attempt = 0;
    loop {
        attempt += 1;
        info!("Sync attempt {}", attempt);
        match fetch_ntp_time(stack).await {
            Ok(unix_seconds) => {
                info!("Initial sync successful: unix_seconds={}", unix_seconds.as_i64());

                sync_events.signal(TimeSyncEvent::SyncSuccess { unix_seconds });
                break;
            }
            Err(e) => {
                info!("Sync failed: {}", e);
                sync_events.signal(TimeSyncEvent::SyncFailed(e));
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

                sync_events.signal(TimeSyncEvent::SyncSuccess { unix_seconds });
                last_success_elapsed = 0; // reset backoff
            }
            Err(e) => {
                info!("Periodic sync failed: {}", e);
                sync_events.signal(TimeSyncEvent::SyncFailed(e));
                info!("Sync failed, will retry in 5 minutes");
            }
        }
    }
}

// ============================================================================
// Network - NTP Fetch
// ============================================================================

async fn fetch_ntp_time(stack: &Stack<'static>) -> Result<UnixSeconds, &'static str> {
    use dns::DnsQueryType;
    use udp::UdpSocket;

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
    let mut rx_meta = [udp::PacketMetadata::EMPTY; 1];
    let mut rx_buffer = [0; 128];
    let mut tx_meta = [udp::PacketMetadata::EMPTY; 1];
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
