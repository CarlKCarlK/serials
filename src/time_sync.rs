//! A device abstraction for Network Time Protocol (NTP) time synchronization over WiFi.
//!
//! This version uses an existing network stack (e.g., from [`WifiSetup`](crate::wifi_setup::WifiSetup)).
//!
//! See [`TimeSync`] for usage examples.

#![allow(clippy::future_not_send, reason = "single-threaded")]

#[cfg(feature = "wifi")]
mod wifi_impl {
    use core::convert::Infallible;
    use defmt::*;
    use embassy_executor::Spawner;
    use embassy_net::{Stack, dns, udp};
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::signal::Signal;
    use embassy_time::{Duration, Timer};
    use static_cell::StaticCell;

    use crate::Result;
    use crate::unix_seconds::UnixSeconds;

    // ============================================================================
    // Types
    // ============================================================================

    /// Events emitted by [`TimeSync`]. See the [`TimeSync`] documentation for usage details.
    #[derive(Clone)]
    pub enum TimeSyncEvent {
        Success { unix_seconds: UnixSeconds },
        // cmk consider changing to Error type?
        Failed(&'static str),
    }

    /// Signal type used by [`TimeSync`] to publish events (see [`TimeSync`] docs).
    type TimeSyncEvents = Signal<CriticalSectionRawMutex, TimeSyncEvent>;

    /// Resources needed to construct a [`TimeSync`] (see [`TimeSync`] docs).
    pub struct TimeSyncStatic {
        events: TimeSyncEvents,
        time_sync_cell: StaticCell<TimeSync>,
    }

    // ============================================================================
    // TimeSync Virtual Device
    // ============================================================================

    /// Device abstraction that manages Network Time Protocol (NTP) synchronization over WiFi.
    ///
    /// Uses an existing network stack (typically from [`WifiSetup`](crate::wifi_setup::WifiSetup)).
    ///
    /// # Sync Timing
    ///
    /// - **Initial sync**: Fires immediately on start (retries at 10s, 30s, 60s, then 5min intervals if failed)
    /// - **Periodic sync**: After first success, syncs every hour (retries every 5min on failure)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # use panic_probe as _;
    /// use embassy_executor::Spawner;
    /// use embassy_net::Stack;
    /// use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
    ///
    /// # #[allow(dead_code)]
    /// async fn run_time_sync(
    ///     stack: &'static Stack<'static>,
    ///     spawner: Spawner,
    /// ) {
    ///     // Create TimeSync with an existing network stack (often from WifiSetup)
    ///     static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    ///     let time_sync = TimeSync::new(&TIME_SYNC_STATIC, stack, spawner);
    ///
    ///     // Wait for sync events
    ///     loop {
    ///         match time_sync.wait().await {
    ///             TimeSyncEvent::Success { unix_seconds } => {
    ///                 defmt::info!("Time synced: {} seconds", unix_seconds.as_i64());
    ///             }
    ///             TimeSyncEvent::Failed(message) => {
    ///                 defmt::info!("time sync failed: {}. Will continue trying", message);
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub struct TimeSync {
        events: &'static TimeSyncEvents,
    }

    impl TimeSync {
        /// Create [`TimeSync`] resources. See [`TimeSync`] docs for usage.
        #[must_use]
        pub const fn new_static() -> TimeSyncStatic {
            TimeSyncStatic {
                events: Signal::new(),
                time_sync_cell: StaticCell::new(),
            }
        }

        /// Create a [`TimeSync`] that uses an existing Embassy stack.
        ///
        /// WiFi is managed elsewhere (e.g. via [`WifiSetup`](crate::wifi_setup::WifiSetup))
        /// and the networking stack is already initialized in client mode.
        pub fn new(
            time_sync_static: &'static TimeSyncStatic,
            stack: &'static Stack<'static>,
            spawner: Spawner,
        ) -> &'static Self {
            let token = unwrap!(time_sync_stack_loop(stack, &time_sync_static.events));
            spawner.spawn(token);

            time_sync_static.time_sync_cell.init(Self {
                events: &time_sync_static.events,
            })
        }

        /// Wait for and return the next [`TimeSyncEvent`]. See [`TimeSync`] docs for an example.
        pub async fn wait(&self) -> TimeSyncEvent {
            self.events.wait().await
        }
    }

    #[embassy_executor::task]
    async fn time_sync_stack_loop(
        stack: &'static Stack<'static>,
        sync_events: &'static TimeSyncEvents,
    ) -> ! {
        let err = run_time_sync_loop(stack, sync_events).await.unwrap_err();
        core::panic!("{err}");
    }

    async fn run_time_sync_loop(
        stack: &'static Stack<'static>,
        sync_events: &'static TimeSyncEvents,
        // cmk Infallible to ! everywhere
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
    // Network - Network Time Protocol (NTP) Fetch
    // ============================================================================

    async fn fetch_ntp_time(stack: &Stack<'static>) -> Result<UnixSeconds, &'static str> {
        use dns::DnsQueryType;
        use udp::UdpSocket;

        // Network Time Protocol (NTP) server configuration
        const NTP_SERVER: &str = "pool.ntp.org";
        const NTP_PORT: u16 = 123;

        // DNS lookup
        info!(
            "Resolving Network Time Protocol (NTP) host {}...",
            NTP_SERVER
        );
        let dns_result = stack
            .dns_query(NTP_SERVER, DnsQueryType::A)
            .await
            .map_err(|e| {
                warn!("DNS lookup failed: {:?}", e);
                "DNS lookup failed"
            })?;
        let server_addr = dns_result.first().ok_or("No DNS results")?;

        info!("Network Time Protocol (NTP) server IP: {}", server_addr);

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

        // Build Network Time Protocol (NTP) request (48 bytes, version 3, client mode)
        let mut ntp_request = [0u8; 48];
        ntp_request[0] = 0x1B; // LI=0, VN=3, Mode=3 (client)

        // Send request
        info!(
            "Sending Network Time Protocol (NTP) request to {}...",
            server_addr
        );
        socket
            .send_to(&ntp_request, (*server_addr, NTP_PORT))
            .await
            .map_err(|e| {
                warn!("Network Time Protocol (NTP) send failed: {:?}", e);
                "Network Time Protocol (NTP) send failed"
            })?;

        // Receive response with timeout
        let mut response = [0u8; 48];
        let (n, _from) =
            embassy_time::with_timeout(Duration::from_secs(5), socket.recv_from(&mut response))
                .await
                .map_err(|_| {
                    warn!("Network Time Protocol (NTP) receive timeout");
                    "Network Time Protocol (NTP) receive timeout"
                })?
                .map_err(|e| {
                    warn!("Network Time Protocol (NTP) receive failed: {:?}", e);
                    "Network Time Protocol (NTP) receive failed"
                })?;

        if n < 48 {
            warn!(
                "Network Time Protocol (NTP) response too short: {} bytes",
                n
            );
            return Err("Network Time Protocol (NTP) response too short");
        }

        // Extract Network Time Protocol (NTP) transmit timestamp (bytes 40-47, big-endian)
        let ntp_seconds =
            u32::from_be_bytes([response[40], response[41], response[42], response[43]]);

        // Convert Network Time Protocol (NTP) timestamp to Unix seconds
        let unix_time = UnixSeconds::from_ntp_seconds(ntp_seconds)
            .ok_or("Invalid Network Time Protocol (NTP) timestamp")?;

        info!(
            "Network Time Protocol (NTP) time: {} (unix timestamp)",
            unix_time.as_i64()
        );
        Ok(unix_time)
    }
} // end wifi_impl module

// Export wifi_impl types when wifi feature is enabled
#[cfg(feature = "wifi")]
pub use wifi_impl::{TimeSync, TimeSyncEvent, TimeSyncStatic};

// ============================================================================
// No-WiFi Stub Implementation
// ============================================================================

#[cfg(not(feature = "wifi"))]
mod stub {
    use crate::unix_seconds::UnixSeconds;
    use embassy_executor::Spawner;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::signal::Signal;
    use static_cell::StaticCell;

    /// Events produced by [`TimeSync`] (see [`TimeSync`] docs for context).
    #[derive(Clone)]
    pub enum TimeSyncEvent {
        Success { unix_seconds: UnixSeconds },
        Failed(&'static str),
    }

    /// Signal type that mirrors the WiFi implementation (see [`TimeSync`] docs).
    type TimeSyncEvents = Signal<CriticalSectionRawMutex, TimeSyncEvent>;

    /// Static used to construct a [`TimeSync`] instance (see [`TimeSync`] docs).
    pub struct TimeSyncStatic {
        events: TimeSyncEvents,
        time_sync_cell: StaticCell<TimeSync>,
    }

    /// Minimal [`TimeSync`] stub that never produces events. See the WiFi [`TimeSync`] docs for examples.
    pub struct TimeSync {
        events: &'static TimeSyncEvents,
    }

    impl TimeSync {
        /// Create [`TimeSync`] resources (see [`TimeSync`] docs for the full device setup).
        #[must_use]
        pub const fn new_static() -> TimeSyncStatic {
            TimeSyncStatic {
                events: Signal::new(),
                time_sync_cell: StaticCell::new(),
            }
        }

        /// Construct the stub device and retain compatibility with [`TimeSync`] docs.
        pub fn new(time_sync_static: &'static TimeSyncStatic, _spawner: Spawner) -> &'static Self {
            time_sync_static.time_sync_cell.init(Self {
                events: &time_sync_static.events,
            })
        }

        /// Wait for the next [`TimeSyncEvent`]. This stub never signals, so waits forever (disabling sync).
        pub async fn wait(&self) -> TimeSyncEvent {
            self.events.wait().await
        }
    }
}

#[cfg(not(feature = "wifi"))]
pub use stub::{TimeSync, TimeSyncEvent, TimeSyncStatic};
