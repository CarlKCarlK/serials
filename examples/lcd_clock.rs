//! LCD Clock - Event-driven time display

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_net::{Config, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::pubsub::PubSubChannel;
use embassy_time::{Duration, Timer};
use heapless::String;
use lib::{CharLcd, LcdChannel, Result};
use panic_probe as _;
use static_cell::StaticCell;

// ============================================================================
// Main Orchestrator
// ============================================================================

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    // Read configuration from compile-time environment (set by build.rs)
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");
    const UTC_OFFSET_MINUTES: &str = env!("UTC_OFFSET_MINUTES");

    info!("Starting LCD Clock (Event-Driven)");
    info!("UTC Offset: {} minutes", UTC_OFFSET_MINUTES);

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Initialize LCD (GP4=SDA, GP5=SCL) - Main owns the LCD
    static LCD_CHANNEL: LcdChannel = CharLcd::channel();
    let lcd = CharLcd::new(p.I2C0, p.PIN_5, p.PIN_4, &LCD_CHANNEL, spawner)?;

    // Initialize PIO for WiFi communication
    let fw = cyw43_firmware::CYW43_43439A0;
    let clm = cyw43_firmware::CYW43_43439A0_CLM;

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(wifi_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // Configure DHCP
    let config = Config::dhcpv4(Default::default());

    // Network stack seed (fixed value is fine for this application)
    let seed = 0x7c8f_3a2e_9d14_6b5a;

    // Init network stack
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

    // Parse offset parameters
    let utc_offset_minutes: i32 = UTC_OFFSET_MINUTES.parse().unwrap_or(0);

    // Spawn clock and time syncer tasks
    unwrap!(spawner.spawn(clock_task()));
    unwrap!(spawner.spawn(time_syncer_task(stack, utc_offset_minutes)));

    // Subscribe to events
    let mut clock_sub = CLOCK_EVENT_BUS.subscriber().unwrap();
    let mut sync_sub = SYNC_EVENT_BUS.subscriber().unwrap();

    info!("Entering main event loop");

    // Main orchestrator loop - owns LCD and paints on events
    loop {
        match select(clock_sub.next_message_pure(), sync_sub.next_message_pure()).await {
            Either::First(ClockEvent::TimeTick(time_info)) => {
                // Format time as 12-hour with AM/PM
                let (hour12, am_pm) = if time_info.hours == 0 {
                    (12, "AM")
                } else if time_info.hours < 12 {
                    (time_info.hours, "AM")
                } else if time_info.hours == 12 {
                    (12, "PM")
                } else {
                    #[expect(clippy::arithmetic_side_effects, reason = "hour guaranteed 13-23")]
                    (time_info.hours - 12, "PM")
                };

                let mut text = String::<64>::new();
                match time_info.state {
                    TimeState::NotSet => {
                        core::fmt::Write::write_fmt(
                            &mut text,
                            format_args!(
                                "{:2}:{:02}:{:02} {}\nTime not set",
                                hour12,
                                time_info.minutes,
                                time_info.seconds,
                                am_pm
                            ),
                        )
                        .unwrap();
                    }
                    TimeState::Synced => {
                        core::fmt::Write::write_fmt(
                            &mut text,
                            format_args!(
                                "{:2}:{:02}:{:02} {}\n{}",
                                hour12,
                                time_info.minutes,
                                time_info.seconds,
                                am_pm,
                                time_info.date_iso.as_str()
                            ),
                        )
                        .unwrap();
                    }
                }
                lcd.display(text, 0);
            }
            Either::Second(TimeSyncEvent::SyncSuccess(time_info)) => {
                info!(
                    "Sync successful: {:02}:{:02}:{:02}",
                    time_info.hours, time_info.minutes, time_info.seconds
                );
                lcd.display(String::<64>::try_from("Synced!").unwrap(), 800);
            }
            Either::Second(TimeSyncEvent::SyncFailed(err)) => {
                info!("Sync failed: {}", err);
                lcd.display(String::<64>::try_from("Sync failed").unwrap(), 800);
            }
        }
    }
}


bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

// ============================================================================
// Types
// ============================================================================

#[derive(Clone, Copy)]
pub enum TimeState {
    NotSet,
    Synced,
}

#[derive(Clone)]
pub struct TimeInfo {
    pub unix: u64,
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub offset_minutes: i32,
    pub date_iso: String<16>,
    pub state: TimeState,
}

pub enum ClockCommand {
    SetTime {
        unix: u64,
        offset_minutes: i32,
        date_iso: String<16>,
    },
}

#[derive(Clone)]
pub enum ClockEvent {
    TimeTick(TimeInfo),
}

#[derive(Clone)]
pub enum TimeSyncEvent {
    SyncSuccess(TimeInfo),
    SyncFailed(&'static str),
}

// ============================================================================
// Channels & Notifiers
// ============================================================================

static CLOCK_CMD_CHANNEL: Channel<CriticalSectionRawMutex, ClockCommand, 4> = Channel::new();
static CLOCK_EVENT_BUS: PubSubChannel<CriticalSectionRawMutex, ClockEvent, 2, 2, 1> =
    PubSubChannel::new();
static SYNC_EVENT_BUS: PubSubChannel<CriticalSectionRawMutex, TimeSyncEvent, 2, 2, 1> =
    PubSubChannel::new();

// ============================================================================
// Clock Task
// ============================================================================

#[embassy_executor::task]
async fn clock_task() -> ! {
    let mut unix_utc: u64 = 0;
    let mut offset_minutes: i32 = 0;
    let mut date_iso: String<16> = String::new();
    let mut time_state = TimeState::NotSet;

    let clock_pub = CLOCK_EVENT_BUS.publisher().unwrap();

    info!("Clock task started");

    loop {
        // Compute current local time
        #[expect(clippy::cast_possible_wrap, reason = "offset is always small")]
        let local_seconds = unix_utc.wrapping_add((offset_minutes * 60) as u64);

        #[expect(clippy::cast_possible_truncation, reason = "seconds in day < u32::MAX")]
        let seconds_in_day = (local_seconds % 86400) as u32;

        #[expect(clippy::arithmetic_side_effects, reason = "bounded time arithmetic")]
        {
            let hours = (seconds_in_day / 3600) as u8;
            let minutes = ((seconds_in_day % 3600) / 60) as u8;
            let seconds = (seconds_in_day % 60) as u8;

            let time_info = TimeInfo {
                unix: unix_utc,
                hours,
                minutes,
                seconds,
                offset_minutes,
                date_iso: date_iso.clone(),
                state: time_state,
            };

            // Emit tick event
            clock_pub.publish_immediate(ClockEvent::TimeTick(time_info));
        }

        // Wait for either 1 second or a command
        match select(Timer::after_secs(1), CLOCK_CMD_CHANNEL.receive()).await {
            Either::First(_) => {
                // Timer elapsed - increment time
                unix_utc = unix_utc.wrapping_add(1);
            }
            Either::Second(cmd) => {
                // Command received
                match cmd {
                    ClockCommand::SetTime {
                        unix,
                        offset_minutes: offset,
                        date_iso: date,
                    } => {
                        unix_utc = unix;
                        offset_minutes = offset;
                        date_iso = date;
                        time_state = TimeState::Synced;
                        info!(
                            "Clock time set: unix={} offset={} date={}",
                            unix, offset, date_iso.as_str()
                        );

                        // Emit immediate tick with new time
                        #[expect(clippy::cast_possible_wrap, reason = "offset is always small")]
                        let local_seconds = unix_utc.wrapping_add((offset_minutes * 60) as u64);

                        #[expect(clippy::cast_possible_truncation, reason = "seconds in day < u32::MAX")]
                        let seconds_in_day = (local_seconds % 86400) as u32;

                        #[expect(clippy::arithmetic_side_effects, reason = "bounded time arithmetic")]
                        {
                            let hours = (seconds_in_day / 3600) as u8;
                            let minutes = ((seconds_in_day % 3600) / 60) as u8;
                            let seconds = (seconds_in_day % 60) as u8;

                            let time_info = TimeInfo {
                                unix: unix_utc,
                                hours,
                                minutes,
                                seconds,
                                offset_minutes,
                                date_iso: date_iso.clone(),
                                state: time_state,
                            };

                            clock_pub.publish_immediate(ClockEvent::TimeTick(time_info));
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Time Syncer Task
// ============================================================================

#[embassy_executor::task]
async fn time_syncer_task(
    stack: &'static embassy_net::Stack<'static>,
    utc_offset_minutes: i32,
) -> ! {
    let sync_pub = SYNC_EVENT_BUS.publisher().unwrap();

    info!("TimeSyncer task started (UTC offset: {} minutes)", utc_offset_minutes);

    // Initial sync with retry (exponential backoff: 10s, 30s, 60s, then 5min intervals)
    let mut attempt = 0;
    loop {
        attempt += 1;
        info!("Sync attempt {}", attempt);
        match fetch_ntp_time(stack).await {
            Ok(unix) => {
                let date_iso = compute_date_string(unix, utc_offset_minutes);

                CLOCK_CMD_CHANNEL
                    .send(ClockCommand::SetTime {
                        unix,
                        offset_minutes: utc_offset_minutes,
                        date_iso: date_iso.clone(),
                    })
                    .await;

                // Compute TimeInfo for the event
                #[expect(clippy::cast_possible_wrap, reason = "offset is always small")]
                let local_seconds = unix.wrapping_add((utc_offset_minutes * 60) as u64);

                #[expect(clippy::cast_possible_truncation, reason = "seconds in day < u32::MAX")]
                let seconds_in_day = (local_seconds % 86400) as u32;

                #[expect(clippy::arithmetic_side_effects, reason = "bounded time arithmetic")]
                let time_info = TimeInfo {
                    unix,
                    hours: (seconds_in_day / 3600) as u8,
                    minutes: ((seconds_in_day % 3600) / 60) as u8,
                    seconds: (seconds_in_day % 60) as u8,
                    offset_minutes: utc_offset_minutes,
                    date_iso,
                    state: TimeState::Synced,
                };

                sync_pub.publish_immediate(TimeSyncEvent::SyncSuccess(time_info));
                break;
            }
            Err(e) => {
                sync_pub.publish_immediate(TimeSyncEvent::SyncFailed(e));
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

        info!("Periodic sync ({}s since last success)...", last_success_elapsed);
        match fetch_ntp_time(stack).await {
            Ok(unix) => {
                let date_iso = compute_date_string(unix, utc_offset_minutes);

                CLOCK_CMD_CHANNEL
                    .send(ClockCommand::SetTime {
                        unix,
                        offset_minutes: utc_offset_minutes,
                        date_iso: date_iso.clone(),
                    })
                    .await;

                // Compute TimeInfo for the event
                #[expect(clippy::cast_possible_wrap, reason = "offset is always small")]
                let local_seconds = unix.wrapping_add((utc_offset_minutes * 60) as u64);

                #[expect(clippy::cast_possible_truncation, reason = "seconds in day < u32::MAX")]
                let seconds_in_day = (local_seconds % 86400) as u32;

                #[expect(clippy::arithmetic_side_effects, reason = "bounded time arithmetic")]
                let time_info = TimeInfo {
                    unix,
                    hours: (seconds_in_day / 3600) as u8,
                    minutes: ((seconds_in_day % 3600) / 60) as u8,
                    seconds: (seconds_in_day % 60) as u8,
                    offset_minutes: utc_offset_minutes,
                    date_iso,
                    state: TimeState::Synced,
                };

                sync_pub.publish_immediate(TimeSyncEvent::SyncSuccess(time_info));
                last_success_elapsed = 0; // Reset counter on success
            }
            Err(e) => {
                sync_pub.publish_immediate(TimeSyncEvent::SyncFailed(e));
                info!("Sync failed, will retry in 5 minutes");
            }
        }
    }
}

// ============================================================================
// Network - NTP Fetch
// ============================================================================

async fn fetch_ntp_time(stack: &embassy_net::Stack<'static>) -> Result<u64, &'static str> {
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
    let (n, _from) = embassy_time::with_timeout(
        Duration::from_secs(5),
        socket.recv_from(&mut response),
    )
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
    let seconds = u32::from_be_bytes([response[40], response[41], response[42], response[43]]);

    // NTP epoch is 1900-01-01, Unix epoch is 1970-01-01
    // Difference: 70 years * 365.25 days/year * 86400 seconds/day = 2208988800
    const NTP_TO_UNIX_OFFSET: u64 = 2208988800;

    if (seconds as u64) < NTP_TO_UNIX_OFFSET {
        warn!("Invalid NTP timestamp: {}", seconds);
        return Err("Invalid NTP timestamp");
    }

    let unix_time = (seconds as u64) - NTP_TO_UNIX_OFFSET;

    info!("NTP time: {} (unix timestamp)", unix_time);
    Ok(unix_time)
}

// ============================================================================
// DST and Date Computation
// ============================================================================

/// Compute month and day from days since Unix epoch
fn compute_month_day(days_since_epoch: u64) -> (u8, u8) {
    // Simplified calendar computation
    const DAYS_PER_MONTH: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    // Approximate year (ignoring leap years for simplicity)
    let days_per_year = 365;
    let _years_since_epoch = days_since_epoch / days_per_year;
    let days_in_year = (days_since_epoch % days_per_year) as u16;

    let mut day_count = 0_u16;
    for (month_idx, &days_in_month) in DAYS_PER_MONTH.iter().enumerate() {
        if day_count + days_in_month > days_in_year {
            let day = (days_in_year - day_count + 1) as u8;
            return ((month_idx + 1) as u8, day);
        }
        day_count += days_in_month;
    }

    (12, 31) // Fallback
}

/// Compute date string "YYYY-MM-DD" from unix timestamp and offset
fn compute_date_string(unix: u64, offset_minutes: i32) -> String<16> {
    #[expect(clippy::cast_possible_wrap, reason = "offset is always small")]
    let local_seconds = unix.wrapping_add((offset_minutes * 60) as u64);

    let days_since_epoch = local_seconds / 86400;

    // Unix epoch: 1970-01-01
    // Simplified year computation
    const DAYS_PER_YEAR: u64 = 365;
    let year = 1970 + (days_since_epoch / DAYS_PER_YEAR);
    let days_in_year = days_since_epoch % DAYS_PER_YEAR;

    let (month, day) = compute_month_day(days_in_year);

    let mut date = String::<16>::new();
    core::fmt::write(&mut date, format_args!("{:04}-{:02}-{:02}", year, month, day)).unwrap();
    date
}

// ============================================================================
// WiFi Tasks
// ============================================================================

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

