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
use embassy_time::Duration;
use heapless::String;
use panic_probe as _;
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::unix_seconds::UnixSeconds;
use serials::wifi_auto::fields::{
    TextField, TextFieldStatic, TimezoneField, TimezoneFieldStatic,
};
use serials::wifi_auto::{WifiAuto, WifiAutoEvent, WifiAutoStatic};
use serials::Result;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting wifi_auto example");
    let peripherals = embassy_rp::init(Default::default());

    static FLASH_STATIC: FlashArrayStatic = FlashArray::<4>::new_static();
    let [wifi_credentials_flash, timezone_flash, device_name_flash, location_flash] =
        FlashArray::new(&FLASH_STATIC, peripherals.FLASH)?;

    static TIMEZONE_FIELD_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
    let timezone_field = TimezoneField::new(&TIMEZONE_FIELD_STATIC, timezone_flash);

    static DEVICE_NAME_FIELD_STATIC: TextFieldStatic<32> = TextField::new_static();
    let device_name_field = TextField::new(
        &DEVICE_NAME_FIELD_STATIC,
        device_name_flash,
        "device_name",
        "Device Name",
        "PicoClock",
    );

    static LOCATION_FIELD_STATIC: TextFieldStatic<64> = TextField::new_static();
    let location_field = TextField::new(
        &LOCATION_FIELD_STATIC,
        location_flash,
        "location",
        "Location",
        "Living Room",
    );

    static WIFI_AUTO_STATIC: WifiAutoStatic = WifiAuto::new_static();
    let wifi_auto = WifiAuto::new(
        &WIFI_AUTO_STATIC,
        peripherals.PIN_23,     // CYW43 power
        peripherals.PIN_25,     // CYW43 chip select
        peripherals.PIO0,       // CYW43 PIO interface
        peripherals.PIN_24,     // CYW43 clock
        peripherals.PIN_29,     // CYW43 data pin
        peripherals.DMA_CH0,    // CYW43 DMA channel
        wifi_credentials_flash, // Flash block storing Wi-Fi creds
        peripherals.PIN_13,     // Reset button pin
        "Pico",                 // Captive-portal SSID to display
        [timezone_field, device_name_field, location_field],
        spawner,
    )?;

    let (stack, mut button) = wifi_auto
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

    let timezone_offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
    let device_name = device_name_field.text()?.unwrap_or_else(|| {
        let mut name = String::new();
        let _ = name.push_str("PicoClock");
        name
    });
    let location = location_field.text()?.unwrap_or_else(|| {
        let mut name = String::new();
        let _ = name.push_str("Living Room");
        name
    });
    info!(
        "Device '{}' in '{}' configured with timezone offset {} minutes",
        device_name, location, timezone_offset_minutes
    );

    // At this point, `stack` can be used for internet access (HTTP, MQTT, etc.)
    // and `button` can be used for user interactions (e.g., triggering actions).
    info!("WiFi setup complete - press button to fetch NTP time");
    loop {
        button.wait_for_press().await;
        match fetch_ntp_time(stack).await {
            Ok(unix_seconds) => info!("Current time: {}", unix_seconds.as_i64()),
            Err(err) => warn!("Failed to fetch time: {}", err),
        }
    }
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
