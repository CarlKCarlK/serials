//! A device abstraction for time synchronization using NTP over WiFi.

#![allow(clippy::future_not_send, reason = "single-threaded")]

#[cfg(feature = "wifi")]
mod wifi_impl {
    use core::convert::Infallible;
    use defmt::*;
    use embassy_executor::Spawner;
    use embassy_net::{Stack, dns, udp};
    use embassy_rp::Peri;
    use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_24, PIN_25, PIN_29, PIO0};
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::signal::Signal;
    use embassy_time::{Duration, Timer};
    use static_cell::StaticCell;

    use crate::Result;
    use crate::flash_array::FlashBlock;
    use crate::unix_seconds::UnixSeconds;
    use crate::wifi::{Wifi, WifiEvent, WifiStatic};

    // ============================================================================
    // Types
    // ============================================================================

    /// Events emitted by the time synchronization device.
    #[derive(Clone)]
    pub enum TimeSyncEvent {
        Success { unix_seconds: UnixSeconds },
        // cmk consider changing to Error type?
        Failed(&'static str),
    }

    /// Signal type for time synchronization events.
    pub type TimeSyncEvents = Signal<CriticalSectionRawMutex, TimeSyncEvent>;

    /// Resources needed by the `TimeSync` device.
    pub struct TimeSyncStatic {
        events: TimeSyncEvents,
        wifi: WifiStatic,
        time_sync_cell: StaticCell<TimeSync>,
    }

    // ============================================================================
    // TimeSync Virtual Device
    // ============================================================================

    /// A device abstraction that manages NTP-based time synchronization over WiFi.
    pub struct TimeSync {
        events: &'static TimeSyncEvents,
        #[allow(dead_code, reason = "Keeps WiFi alive or holds stack provider")]
        wifi: Option<&'static Wifi>,
    }

    impl TimeSync {
        /// Create TimeSync resources (includes WiFi)
        #[must_use]
        pub const fn new_static() -> TimeSyncStatic {
            TimeSyncStatic {
                events: Signal::new(),
                wifi: Wifi::new_static(),
                time_sync_cell: StaticCell::new(),
            }
        }

        /// Create a new TimeSync device (creates WiFi internally) and spawn its task
        ///
        /// # Arguments
        /// * `credential_store` - Flash block for persisted WiFi credentials
        pub fn new(
            time_sync_static: &'static TimeSyncStatic,
            pin_23: Peri<'static, PIN_23>,
            pin_25: Peri<'static, PIN_25>,
            pio0: Peri<'static, PIO0>,
            pin_24: Peri<'static, PIN_24>,
            pin_29: Peri<'static, PIN_29>,
            dma_ch0: Peri<'static, DMA_CH0>,
            credential_store: FlashBlock,
            spawner: Spawner,
        ) -> &'static Self {
            // Create WiFi device
            let wifi = Wifi::new(
                &time_sync_static.wifi,
                pin_23,
                pin_25,
                pio0,
                pin_24,
                pin_29,
                dma_ch0,
                credential_store,
                spawner,
            );

            // Spawn TimeSync task
            let token = unwrap!(time_sync_device_loop(wifi, &time_sync_static.events));
            spawner.spawn(token);

            time_sync_static.time_sync_cell.init(Self {
                events: &time_sync_static.events,
                wifi: Some(wifi),
            })
        }

        /// Create a new TimeSync device that uses an existing Embassy stack instead of
        /// provisioning WiFi hardware.
        ///
        /// This is useful when WiFi is managed elsewhere (e.g. via [`WifiAuto`](crate::wifi_auto::WifiAuto))
        /// and the networking stack is already initialized in client mode.
        pub fn new_from_stack(
            time_sync_static: &'static TimeSyncStatic,
            stack: &'static Stack<'static>,
            spawner: Spawner,
        ) -> &'static Self {
            let token = unwrap!(time_sync_stack_loop(stack, &time_sync_static.events));
            spawner.spawn(token);

            time_sync_static.time_sync_cell.init(Self {
                events: &time_sync_static.events,
                wifi: None,
            })
        }

        /// Get reference to WiFi device (useful for captive portal configuration)
        pub fn wifi(&self) -> &'static Wifi {
            self.wifi
                .expect("TimeSync WiFi handle unavailable (stack-based mode)")
        }

        /// Wait for and return the next TimeSync event
        pub async fn wait(&self) -> TimeSyncEvent {
            self.events.wait().await
        }
    }

    #[embassy_executor::task]
    async fn time_sync_device_loop(wifi: &'static Wifi, sync_events: &'static TimeSyncEvents) -> ! {
        let err = inner_time_sync_device_loop(wifi, sync_events)
            .await
            .unwrap_err();
        core::panic!("{err}");
    }

    #[embassy_executor::task]
    async fn time_sync_stack_loop(
        stack: &'static Stack<'static>,
        sync_events: &'static TimeSyncEvents,
    ) -> ! {
        let err = run_time_sync_loop(stack, sync_events).await.unwrap_err();
        core::panic!("{err}");
    }

    async fn inner_time_sync_device_loop(
        wifi: &'static Wifi,
        sync_events: &'static TimeSyncEvents,
    ) -> Result<Infallible> {
        info!("TimeSync device awaiting network stack...");

        // Wait for WiFi to be ready and get the stack
        let stack = wifi.stack().await;

        // Check what kind of WiFi event we got
        let wifi_event = wifi.wait().await;
        match wifi_event {
            WifiEvent::CaptivePortalReady => {
                info!("TimeSync: WiFi in captive portal mode - waiting for client connection");
                info!("TimeSync: NTP sync will not start until switched to client mode");
                // In captive portal mode, we don't sync time - just wait indefinitely
                loop {
                    Timer::after_secs(3600).await;
                }
            }
            WifiEvent::ClientReady => {
                info!("TimeSync: WiFi in client mode - starting NTP sync");
            }
        }

        run_time_sync_loop(stack, sync_events).await
    }

    async fn run_time_sync_loop(
        stack: &'static Stack<'static>,
        sync_events: &'static TimeSyncEvents,
    ) -> Result<Infallible> {
        info!("TimeSync received network stack");
        info!("TimeSync device started");

        // Initial sync with retry (exponential backoff: 10s, 30s, 60s, then 5min intervals)
        let mut attempt = 0;
        loop {
            attempt += 1;
            info!("Sync attempt {}", attempt);
            match fetch_ntp_time(stack).await {
                Ok(unix_seconds) => {
                    info!(
                        "Initial sync successful: unix_seconds={}",
                        unix_seconds.as_i64()
                    );

                    sync_events.signal(TimeSyncEvent::Success { unix_seconds });
                    break;
                }
                Err(e) => {
                    info!("Sync failed: {}", e);
                    sync_events.signal(TimeSyncEvent::Failed(e));
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
                    info!(
                        "Periodic sync successful: unix_seconds={}",
                        unix_seconds.as_i64()
                    );

                    sync_events.signal(TimeSyncEvent::Success { unix_seconds });
                    last_success_elapsed = 0; // reset backoff
                }
                Err(e) => {
                    info!("Periodic sync failed: {}", e);
                    sync_events.signal(TimeSyncEvent::Failed(e));
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
        let ntp_seconds =
            u32::from_be_bytes([response[40], response[41], response[42], response[43]]);

        // Convert NTP timestamp to Unix seconds
        let unix_time =
            UnixSeconds::from_ntp_seconds(ntp_seconds).ok_or("Invalid NTP timestamp")?;

        info!("NTP time: {} (unix timestamp)", unix_time.as_i64());
        Ok(unix_time)
    }
} // end wifi_impl module

// Export wifi_impl types when wifi feature is enabled
#[cfg(feature = "wifi")]
pub use wifi_impl::{TimeSync, TimeSyncEvent, TimeSyncEvents, TimeSyncStatic};

// ============================================================================
// No-WiFi Stub Implementation
// ============================================================================

#[cfg(not(feature = "wifi"))]
mod stub {
    use crate::unix_seconds::UnixSeconds;
    use core::future::pending;
    use embassy_executor::Spawner;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::signal::Signal;
    use static_cell::StaticCell;

    /// Events produced by the time synchronisation task.
    #[derive(Clone)]
    pub enum TimeSyncEvent {
        Success { unix_seconds: UnixSeconds },
        Failed(&'static str),
    }

    pub type TimeSyncEvents = Signal<CriticalSectionRawMutex, TimeSyncEvent>;

    /// Static used to construct a [`TimeSync`] instance.
    pub struct TimeSyncStatic {
        time_sync_cell: StaticCell<TimeSync>,
    }

    /// Minimal time synchronization stub that never produces events.
    pub struct TimeSync;

    impl TimeSync {
        /// Create time sync resources.
        #[must_use]
        pub const fn new_static() -> TimeSyncStatic {
            TimeSyncStatic {
                time_sync_cell: StaticCell::new(),
            }
        }

        /// Construct the stub device.
        pub fn new(time_sync_static: &'static TimeSyncStatic, _spawner: Spawner) -> &'static Self {
            time_sync_static.time_sync_cell.init(Self {})
        }

        /// Wait for the next time sync event. This stub never resolves, effectively disabling sync.
        pub async fn wait(&self) -> TimeSyncEvent {
            pending().await
        }
    }
}

#[cfg(not(feature = "wifi"))]
pub use stub::{TimeSync, TimeSyncEvent, TimeSyncEvents, TimeSyncStatic};
