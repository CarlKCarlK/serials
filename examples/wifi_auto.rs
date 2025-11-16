//! Minimal example that provisions Wi-Fi credentials using the `WifiAuto`
//! abstraction and displays connection status on a 4-digit LED display.
//!
//! // cmk0 Future iterations should add extra captive-portal widgets (e.g. nickname)
//! // and show how to persist their flash-backed values before handing control back
//! // to the application logic.

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::{info, warn};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_net::{Stack, dns::DnsQueryType, udp};
use embassy_rp::gpio::{self, Level};
use embassy_time::Duration;
use panic_probe as _;
use serials::Result;
use serials::flash_array::{FlashArray, FlashArrayNotifier};
use serials::led4::{AnimationFrame, BlinkState, Led4, Led4Animation, Led4Notifier, OutputArray};
use serials::unix_seconds::UnixSeconds;
use serials::wifi_auto::{
    WifiAutoConfig, WifiAutoConnected, WifiAutoEvent, WifiAutoHandle, WifiAutoNotifier,
};
use serials::wifi_config::Nickname;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting wifi_auto example");
    let peripherals = embassy_rp::init(Default::default());

    static FLASH_NOTIFIER: FlashArrayNotifier = FlashArray::<3>::notifier();
    let [wifi_credentials_flash, timezone_flash, nickname_flash] =
        FlashArray::new(&FLASH_NOTIFIER, peripherals.FLASH)?;

    let cells = OutputArray::new([
        gpio::Output::new(peripherals.PIN_1, Level::High),
        gpio::Output::new(peripherals.PIN_2, Level::High),
        gpio::Output::new(peripherals.PIN_3, Level::High),
        gpio::Output::new(peripherals.PIN_4, Level::High),
    ]);
    let segments = OutputArray::new([
        gpio::Output::new(peripherals.PIN_5, Level::Low),
        gpio::Output::new(peripherals.PIN_6, Level::Low),
        gpio::Output::new(peripherals.PIN_7, Level::Low),
        gpio::Output::new(peripherals.PIN_8, Level::Low),
        gpio::Output::new(peripherals.PIN_9, Level::Low),
        gpio::Output::new(peripherals.PIN_10, Level::Low),
        gpio::Output::new(peripherals.PIN_11, Level::Low),
        gpio::Output::new(peripherals.PIN_12, Level::Low),
    ]);

    static LED4_NOTIFIER: Led4Notifier = Led4::notifier();
    let led4 = Led4::new(cells, segments, &LED4_NOTIFIER, spawner)?;

    static WIFI_AUTO_NOTIFIER: WifiAutoNotifier = WifiAutoHandle::notifier();
    let wifi_auto = WifiAutoHandle::new(
        &WIFI_AUTO_NOTIFIER,
        peripherals.PIN_23,     // CYW43 power
        peripherals.PIN_25,     // CYW43 chip select
        peripherals.PIO0,       // CYW43 PIO interface
        peripherals.PIN_24,     // CYW43 clock
        peripherals.PIN_29,     // CYW43 data pin
        peripherals.DMA_CH0,    // CYW43 DMA channel
        wifi_credentials_flash, // Flash block storing Wi-Fi creds
        peripherals.PIN_13,     // User button pin
        "Pico",                 // Captive-portal SSID to display
        WifiAutoConfig::new()
            .with_timezone(timezone_flash) // Flash block storing timezone offset
            .with_nickname(nickname_flash), // Flash block storing user nickname
        spawner,
    )?;

    let WifiAutoConnected {
        stack,
        mut button,
        mut timezone_flash,
        mut nickname_flash,
    } = wifi_auto
        .ensure_connected_with_ui(spawner, |event| match event {
            WifiAutoEvent::CaptivePortalReady => {
                led4.write_text(BlinkState::BlinkingAndOn, ['C', 'O', 'N', 'N']);
            }

            WifiAutoEvent::ClientConnecting { try_index, .. } => {
                led4.animate_text(circular_outline_animation((try_index & 1) == 0));
            }

            WifiAutoEvent::Connected => {
                led4.write_text(BlinkState::Solid, ['D', 'O', 'N', 'E']);
            }
        })
        .await?;

    let mut timezone_flash = timezone_flash.expect("timezone flash not returned");
    let timezone_offset_minutes = timezone_flash.load::<i32>()?.unwrap_or(0);
    let mut nickname_flash = nickname_flash.expect("nickname flash not returned");
    let nickname: Nickname = nickname_flash.load::<Nickname>()?.unwrap_or_else(|| {
        let mut fallback = Nickname::new();
        let _ = fallback.push_str("PicoClock");
        fallback
    });
    info!(
        "Nickname '{}' configured with offset {} minutes",
        nickname, timezone_offset_minutes
    );
    info!("push button");
    loop {
        button.wait_for_press().await;
        match fetch_ntp_time(stack).await {
            Ok(unix_seconds) => info!("Current time: {}", unix_seconds.as_i64()),
            Err(err) => warn!("Failed to fetch time: {}", err),
        }
    }
}

fn circular_outline_animation(clockwise: bool) -> Led4Animation {
    const FRAME_DURATION: Duration = Duration::from_millis(120);
    const CLOCKWISE: [[char; 4]; 8] = [
        ['\'', '\'', '\'', '\''],
        ['\'', '\'', '\'', '"'],
        [' ', ' ', ' ', '>'],
        [' ', ' ', ' ', ')'],
        ['_', '_', '_', '_'],
        ['*', '_', '_', '_'],
        ['<', ' ', ' ', ' '],
        ['(', '\'', '\'', '\''],
    ];
    const COUNTER: [[char; 4]; 8] = [
        ['(', '\'', '\'', '\''],
        ['<', ' ', ' ', ' '],
        ['*', '_', '_', '_'],
        ['_', '_', '_', '_'],
        [' ', ' ', ' ', ')'],
        [' ', ' ', ' ', '>'],
        ['\'', '\'', '\'', '"'],
        ['\'', '\'', '\'', '\''],
    ];

    let mut animation = Led4Animation::new();
    let frames = if clockwise { &CLOCKWISE } else { &COUNTER };
    for text in frames {
        let _ = animation.push(AnimationFrame::new(*text, FRAME_DURATION));
    }
    animation
}

async fn fetch_ntp_time(stack: &'static Stack<'static>) -> Result<UnixSeconds, &'static str> {
    use udp::UdpSocket;

    const NTP_SERVER: &str = "pool.ntp.org";
    const NTP_PORT: u16 = 123;

    info!("Resolving {}...", NTP_SERVER);
    let dns_result = stack
        .dns_query(NTP_SERVER, DnsQueryType::A)
        .await
        .map_err(|e| {
            warn!("DNS lookup failed: {:?}", e);
            "DNS lookup failed"
        })?;
    let server_addr = dns_result.first().ok_or("No DNS results")?;

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

    let mut ntp_request = [0u8; 48];
    ntp_request[0] = 0x1B;
    info!("Sending NTP request...");
    socket
        .send_to(&ntp_request, (*server_addr, NTP_PORT))
        .await
        .map_err(|e| {
            warn!("NTP send failed: {:?}", e);
            "NTP send failed"
        })?;

    let mut response = [0u8; 48];
    let (n, _) =
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

    let ntp_seconds = u32::from_be_bytes([response[40], response[41], response[42], response[43]]);
    UnixSeconds::from_ntp_seconds(ntp_seconds).ok_or("Invalid NTP timestamp")
}
