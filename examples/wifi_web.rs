//! WiFi Web Server AP - host a tiny web page from a soft access point.
//!
//! This example places the Pico W into access-point mode, serves a minimal HTTP page,
//! and keeps all networking on-device. Connect from a phone or laptop directly to the
//! advertised SSID and browse to the logged IP address.
//!
//! Environment overrides (optional):
//!   - `PICO_AP_SSID` for the Wi-Fi name (default: `pico-serials`)
//!   - `PICO_AP_PASSWORD` for the WPA2 password (default: `picoserials`, empty for open AP)
//!   - `PICO_AP_CHANNEL` for the channel number (default: 6)
//!
//! Run with:
//!   - Pico 1 W: `cargo wifi_web_1w`
//!   - Pico 2 W: `cargo wifi_web_2w`

#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::fmt::Write as _;
use core::str::from_utf8;
use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
use defmt::{Debug2Format, debug, error, info, trace, warn};
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{self, UdpSocket};
use embassy_net::{Config, IpAddress, Ipv4Address, Ipv4Cidr, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async::Write as _;
use heapless::String;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

fn ap_ssid() -> &'static str {
    option_env!("PICO_AP_SSID").unwrap_or("pico-serials")
}

fn ap_password() -> &'static str {
    option_env!("PICO_AP_PASSWORD").unwrap_or("")
}

fn ap_channel() -> u8 {
    option_env!("PICO_AP_CHANNEL")
        .and_then(|raw| raw.parse::<u8>().ok())
        .filter(|ch| (1..=11).contains(ch))
        .unwrap_or(6)
}

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;
const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
const DHCP_LEASE_SECONDS: u32 = 4 * 60 * 60; // 4-hour leases keep clients around without being permanent

const DNS_SERVER_PORT: u16 = 53;
const DNS_RESPONSE_TTL: u32 = 60;

struct StaticAsset {
    path: &'static str,
    content_type: &'static str,
    body: &'static [u8],
}

struct CaptivePortalResponse {
    path: &'static str,
    status_line: &'static str,
    content_type: &'static str,
    body: &'static [u8],
    location: Option<&'static str>,
}

const CAPTIVE_PORTAL_BODY: &[u8] = b"<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Busy Beaver Blaze Captive Portal</title><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>body{font-family:Arial,sans-serif;margin:0;padding:2rem;background:#f4f4f4;color:#111;text-align:center;}main{background:#fff;border-radius:8px;max-width:520px;margin:4vh auto;padding:2rem;box-shadow:0 2px 8px rgba(0,0,0,0.1);}main h1{margin-top:0;}main p{margin:0.8rem 0;line-height:1.5;}code{display:inline-block;padding:0.35rem 0.6rem;background:#f1f5f9;border-radius:4px;font-size:0.9rem;}a.button{display:inline-block;margin-top:1.2rem;padding:0.75rem 1.5rem;background:#2563eb;color:#fff;text-decoration:none;border-radius:4px;font-weight:600;}a.button:active{background:#1e40af;}footer{margin-top:1.6rem;font-size:0.85rem;color:#555;}ol{max-width:420px;margin:1.2rem auto;text-align:left;line-height:1.4;padding-left:1.2rem;}ol li{margin:0.35rem 0;}</style></head><body><main><h1>Busy Beaver Blaze</h1><p>This sign-in window cannot run the visualization. Captive portal browsers disable WebAssembly and workers.</p><p>To run the WASM visualizer:</p><ol><li>Choose <strong>Use network as is</strong> or dismiss this window.</li><li>Open Chrome or Firefox.</li><li>Visit <code>http://192.168.4.1/index.html</code> (tap the button below to copy the link).</li></ol><a class=\"button\" href=\"http://192.168.4.1/index.html\">Open in Browser</a><footer>If nothing opens automatically, close this window and visit the link manually.</footer></main></body></html>";

const CAPTIVE_PORTAL_TEXT_BODY: &[u8] =
    b"Sign-in required. Open http://192.168.4.1/portal\r\n";
const CAPTIVE_PORTAL_MSFT_TEXT: &[u8] =
    b"Busy Beaver Blaze captive portal. Visit http://192.168.4.1/portal\r\n";
const CAPTIVE_PORTAL_REDIRECT_BODY: &[u8] =
    b"Redirecting to http://192.168.4.1/portal\r\n";
const PORTAL_LOCATION: &str = "http://192.168.4.1/portal";

static CAPTIVE_PORTAL_RESPONSES: &[CaptivePortalResponse] = &[
    // Note: "/" removed from captive responses so it serves the actual app
    CaptivePortalResponse {
        path: "/portal",
        status_line: "HTTP/1.1 200 OK\r\n",
        content_type: "text/html; charset=utf-8",
        body: CAPTIVE_PORTAL_BODY,
        location: None,
    },
    CaptivePortalResponse {
        path: "/generate_204",
        status_line: "HTTP/1.1 204 No Content\r\n",
        content_type: "text/plain; charset=utf-8",
        body: b"",
        location: None,
    },
    CaptivePortalResponse {
        path: "/gen_204",
        status_line: "HTTP/1.1 204 No Content\r\n",
        content_type: "text/plain; charset=utf-8",
        body: b"",
        location: None,
    },
    CaptivePortalResponse {
        path: "/canonical.html",
        status_line: "HTTP/1.1 204 No Content\r\n",
        content_type: "text/plain; charset=utf-8",
        body: b"",
        location: None,
    },
    CaptivePortalResponse {
        path: "/hotspot-detect.html",
        status_line: "HTTP/1.1 200 OK\r\n",
        content_type: "text/html; charset=utf-8",
        body: b"<!doctype html><html><head><title>Success</title></head><body>Success</body></html>",
        location: None,
    },
    CaptivePortalResponse {
        path: "/library/test/success.html",
        status_line: "HTTP/1.1 200 OK\r\n",
        content_type: "text/html; charset=utf-8",
        body: CAPTIVE_PORTAL_BODY,
        location: None,
    },
    CaptivePortalResponse {
        path: "/connecttest.txt",
        status_line: "HTTP/1.1 200 OK\r\n",
        content_type: "text/plain; charset=utf-8",
        body: b"Microsoft Connect Test",
        location: None,
    },
    CaptivePortalResponse {
        path: "/ncsi.txt",
        status_line: "HTTP/1.1 200 OK\r\n",
        content_type: "text/plain; charset=utf-8",
        body: b"Microsoft NCSI",
        location: None,
    },
    CaptivePortalResponse {
        path: "/check_network_status.txt",
        status_line: "HTTP/1.1 200 OK\r\n",
        content_type: "text/plain; charset=utf-8",
        body: b"NetworkManager is online",
        location: None,
    },
    CaptivePortalResponse {
        path: "/success.txt",
        status_line: "HTTP/1.1 200 OK\r\n",
        content_type: "text/plain; charset=utf-8",
        body: CAPTIVE_PORTAL_TEXT_BODY,
        location: None,
    },
    CaptivePortalResponse {
        path: "/redirect",
        status_line: "HTTP/1.1 302 Found\r\n",
        content_type: "text/plain; charset=utf-8",
        body: CAPTIVE_PORTAL_REDIRECT_BODY,
        location: Some(PORTAL_LOCATION),
    },
];

static STATIC_ASSETS: &[StaticAsset] = &[
    StaticAsset {
        path: "/test.html",
        content_type: "text/html; charset=utf-8",
        body: include_bytes!("../examples/static/busy_beaver_blaze/v0.2.7/test.html"),
    },
    StaticAsset {
        path: "/index.html",
        content_type: "text/html; charset=utf-8",
        body: include_bytes!("../examples/static/busy_beaver_blaze/v0.2.7/index.html"),
    },
    StaticAsset {
        path: "/styles.css",
        content_type: "text/css; charset=utf-8",
        body: include_bytes!("../examples/static/busy_beaver_blaze/v0.2.7/styles.css"),
    },
    StaticAsset {
        path: "/worker.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_bytes!("../examples/static/busy_beaver_blaze/v0.2.7/worker.js"),
    },
    StaticAsset {
        path: "/pkg/busy_beaver_blaze.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_bytes!(
            "../examples/static/busy_beaver_blaze/v0.2.7/pkg/busy_beaver_blaze.js"
        ),
    },
    StaticAsset {
        path: "/pkg/busy_beaver_blaze_bg.wasm",
        content_type: "application/wasm",
        body: include_bytes!(
            "../examples/static/busy_beaver_blaze/v0.2.7/pkg/busy_beaver_blaze_bg.wasm"
        ),
    },
    StaticAsset {
        path: "/favicon.ico",
        content_type: "image/x-icon",
        body: include_bytes!("../examples/static/busy_beaver_blaze/favicon.ico"),
    },
    StaticAsset {
        path: "/busy_beaver_blaze/favicon.ico",
        content_type: "image/x-icon",
        body: include_bytes!("../examples/static/busy_beaver_blaze/favicon.ico"),
    },
];

fn find_asset(path: &str) -> Option<&'static StaticAsset> {
    let normalized = if path == "/" || path.is_empty() {
        "/index.html"
    } else {
        path
    };

    STATIC_ASSETS.iter().find(|asset| asset.path == normalized)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, defmt::Format)]
enum DhcpMessageType {
    Discover,
    Request,
    Decline,
    Release,
    Inform,
    Other(u8),
}

struct DhcpMessage {
    msg_type: DhcpMessageType,
    transaction_id: u32,
    hardware_type: u8,
    hardware_len: u8,
    flags: u16,
    client_mac: [u8; 6],
    client_ip: Option<Ipv4Address>,
    requested_ip: Option<Ipv4Address>,
    server_id: Option<Ipv4Address>,
}

struct DhcpLease {
    mac: [u8; 6],
    ip: Ipv4Address,
    expires_at: Instant,
}

fn parse_dhcp_message(frame: &[u8]) -> Option<DhcpMessage> {
    if frame.len() < 240 {
        return None;
    }

    if frame[0] != 1 {
        // Only process BOOTREQUEST packets
        return None;
    }

    let hardware_type = frame[1];
    let hardware_len = frame[2];
    if hardware_type != 1 || hardware_len != 6 {
        // Only support Ethernet clients with 6-byte MACs for now
        return None;
    }

    let transaction_id = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
    let flags = u16::from_be_bytes([frame[10], frame[11]]);

    if frame[236..240] != DHCP_MAGIC_COOKIE {
        return None;
    }

    let mut msg_type = None;
    let mut requested_ip = None;
    let mut server_id = None;

    let mut idx = 240;
    while idx < frame.len() {
        let opt = frame[idx];
        idx += 1;
        match opt {
            0 => continue,
            255 => break,
            _ => {
                if idx >= frame.len() {
                    break;
                }
                let len = frame[idx] as usize;
                idx += 1;
                if idx + len > frame.len() {
                    break;
                }
                let data = &frame[idx..idx + len];
                match opt {
                    50 if len == 4 => {
                        requested_ip = Some(Ipv4Address::new(data[0], data[1], data[2], data[3]));
                    }
                    53 if len == 1 => {
                        msg_type = Some(match data[0] {
                            1 => DhcpMessageType::Discover,
                            3 => DhcpMessageType::Request,
                            4 => DhcpMessageType::Decline,
                            7 => DhcpMessageType::Release,
                            8 => DhcpMessageType::Inform,
                            other => DhcpMessageType::Other(other),
                        });
                    }
                    54 if len == 4 => {
                        server_id = Some(Ipv4Address::new(data[0], data[1], data[2], data[3]));
                    }
                    _ => {}
                }
                idx += len;
            }
        }
    }

    let ciaddr = Ipv4Address::new(frame[12], frame[13], frame[14], frame[15]);
    let client_ip = if ciaddr == Ipv4Address::UNSPECIFIED {
        None
    } else {
        Some(ciaddr)
    };

    let mut client_mac = [0u8; 6];
    client_mac.copy_from_slice(&frame[28..34]);

    Some(DhcpMessage {
        msg_type: msg_type?,
        transaction_id,
        hardware_type,
        hardware_len,
        flags,
        client_mac,
        client_ip,
        requested_ip,
        server_id,
    })
}

fn append_option(dest: &mut [u8], code: u8, payload: &[u8]) -> Option<usize> {
    let needed = payload.len().saturating_add(2);
    if dest.len() < needed {
        return None;
    }
    dest[0] = code;
    dest[1] = payload.len() as u8;
    dest[2..2 + payload.len()].copy_from_slice(payload);
    Some(needed)
}

fn build_dhcp_reply(
    scratch: &mut [u8],
    request: &DhcpMessage,
    offered_ip: Ipv4Address,
    server_ip: Ipv4Address,
    netmask: Ipv4Address,
    broadcast_ip: Ipv4Address,
    response_kind: DhcpMessageType,
) -> Option<usize> {
    if scratch.len() < 300 {
        return None;
    }

    scratch.fill(0);
    scratch[0] = 2; // BOOTREPLY
    scratch[1] = request.hardware_type;
    scratch[2] = request.hardware_len;
    scratch[3] = 0; // hops
    scratch[4..8].copy_from_slice(&request.transaction_id.to_be_bytes());
    scratch[10..12].copy_from_slice(&request.flags.to_be_bytes());
    scratch[16..20].copy_from_slice(&offered_ip.octets());
    let server_bytes = server_ip.octets();
    let netmask_bytes = netmask.octets();
    let broadcast_bytes = broadcast_ip.octets();

    scratch[20..24].copy_from_slice(&server_bytes);
    scratch[28..34].copy_from_slice(&request.client_mac);
    scratch[236..240].copy_from_slice(&DHCP_MAGIC_COOKIE);

    let lease = DHCP_LEASE_SECONDS;
    let renewal = lease / 2;
    let rebinding = (lease as u64 * 7 / 8) as u32;

    let mut idx = 240;
    idx += append_option(
        &mut scratch[idx..],
        53,
        &[match response_kind {
            DhcpMessageType::Discover => 2, // Offer
            DhcpMessageType::Request => 5,  // Ack
            DhcpMessageType::Other(code) => code,
            DhcpMessageType::Decline => 6,
            DhcpMessageType::Release => 7,
            DhcpMessageType::Inform => 8,
        }],
    )?;
    idx += append_option(&mut scratch[idx..], 54, &server_bytes)?; // Server identifier
    idx += append_option(&mut scratch[idx..], 51, &lease.to_be_bytes())?; // Lease time
    idx += append_option(&mut scratch[idx..], 58, &renewal.to_be_bytes())?; // Renewal (T1)
    idx += append_option(&mut scratch[idx..], 59, &rebinding.to_be_bytes())?; // Rebinding (T2)
    idx += append_option(&mut scratch[idx..], 1, &netmask_bytes)?; // Subnet mask
    idx += append_option(&mut scratch[idx..], 3, &server_bytes)?; // Router
    idx += append_option(&mut scratch[idx..], 6, &server_bytes)?; // DNS server
    idx += append_option(&mut scratch[idx..], 28, &broadcast_bytes)?; // Broadcast address
    scratch[idx] = 255; // End option
    idx += 1;

    Some(idx)
}

fn bump_ipv4(base: Ipv4Address, offset: u8) -> Ipv4Address {
    let base_u32 = u32::from_be_bytes(base.octets());
    let candidate = base_u32.saturating_add(offset as u32);
    let octets = candidate.to_be_bytes();
    Ipv4Address::new(octets[0], octets[1], octets[2], octets[3])
}

fn ip_in_pool(ip: Ipv4Address, pool_start: Ipv4Address, pool_size: u8) -> bool {
    if pool_size == 0 {
        return false;
    }
    let start = u32::from_be_bytes(pool_start.octets());
    let end = start + pool_size as u32 - 1;
    let value = u32::from_be_bytes(ip.octets());
    value >= start && value <= end
}

fn ensure_lease(
    leases: &mut heapless::Vec<DhcpLease, 8>,
    mac: [u8; 6],
    pool_start: Ipv4Address,
    pool_size: u8,
    requested: Option<Ipv4Address>,
) -> Option<Ipv4Address> {
    let now = Instant::now();
    leases.retain(|lease| lease.expires_at > now);

    let expiry = now + Duration::from_secs(DHCP_LEASE_SECONDS as u64);
    let desired_ip = requested
        .filter(|ip| ip_in_pool(*ip, pool_start, pool_size))
        .filter(|ip| {
            leases
                .iter()
                .all(|lease| lease.mac == mac || lease.ip != *ip)
        });

    if let Some(existing) = leases.iter_mut().find(|lease| lease.mac == mac) {
        if let Some(ip) = desired_ip {
            existing.ip = ip;
        }
        existing.expires_at = expiry;
        return Some(existing.ip);
    }

    if let Some(ip) = desired_ip {
        if leases
            .push(DhcpLease {
                mac,
                ip,
                expires_at: expiry,
            })
            .is_ok()
        {
            return Some(ip);
        }
    }

    for idx in 0..pool_size {
        let candidate = bump_ipv4(pool_start, idx);
        if leases.iter().any(|lease| lease.ip == candidate) {
            continue;
        }
        if leases
            .push(DhcpLease {
                mac,
                ip: candidate,
                expires_at: expiry,
            })
            .is_ok()
        {
            return Some(candidate);
        }
    }

    None
}

struct DnsQuestion {
    len: usize,
    qtype: u16,
    #[allow(dead_code)]
    qclass: u16,
    name: heapless::String<253>,
}

fn parse_dns_question(packet: &[u8]) -> Option<DnsQuestion> {
    if packet.len() < 12 {
        return None;
    }

    let mut idx = 12;
    let mut name = heapless::String::<253>::new();

    loop {
        let label_len = *packet.get(idx)? as usize;
        idx += 1;
        if label_len == 0 {
            break;
        }
        if idx + label_len > packet.len() {
            return None;
        }
        let label_bytes = &packet[idx..idx + label_len];
        let label = from_utf8(label_bytes).ok()?;
        if !name.is_empty() {
            name.push('.').ok()?;
        }
        name.push_str(label).ok()?;
        idx += label_len;
    }

    if idx + 4 > packet.len() {
        return None;
    }

    let qtype = u16::from_be_bytes([packet[idx], packet[idx + 1]]);
    let qclass = u16::from_be_bytes([packet[idx + 2], packet[idx + 3]]);
    idx += 4;

    Some(DnsQuestion {
        len: idx - 12,
        qtype,
        qclass,
        name,
    })
}

fn build_dns_response(
    query: &[u8],
    response: &mut [u8],
    answer_ip: Ipv4Address,
    question: &DnsQuestion,
) -> Option<usize> {
    if query.len() < 12 || response.len() < 12 {
        return None;
    }

    let question_end = 12 + question.len;
    if response.len() < question_end {
        return None;
    }

    response.fill(0);
    response[0..2].copy_from_slice(&query[0..2]);
    response[2] = 0x81; // standard response + recursion available
    response[3] = 0x80;
    response[4..6].copy_from_slice(&query[4..6]); // QDCOUNT
    response[6..8].copy_from_slice(&1u16.to_be_bytes());

    // NSCOUNT and ARCOUNT remain zero (already zeroed)

    response[12..question_end].copy_from_slice(&query[12..question_end]);

    let mut offset = question_end;
    if response.len() < offset + 16 {
        return None;
    }

    response[offset] = 0xC0;
    response[offset + 1] = 0x0C; // pointer to question name
    response[offset + 2..offset + 4].copy_from_slice(&1u16.to_be_bytes());
    response[offset + 4..offset + 6].copy_from_slice(&1u16.to_be_bytes());
    response[offset + 6..offset + 10].copy_from_slice(&DNS_RESPONSE_TTL.to_be_bytes());
    response[offset + 10..offset + 12].copy_from_slice(&4u16.to_be_bytes());
    response[offset + 12..offset + 16].copy_from_slice(&answer_ip.octets());
    offset += 16;

    Some(offset)
}

fn message_kind_label(kind: DhcpMessageType) -> &'static str {
    match kind {
        DhcpMessageType::Discover => "DISCOVER",
        DhcpMessageType::Request => "REQUEST",
        DhcpMessageType::Decline => "DECLINE",
        DhcpMessageType::Release => "RELEASE",
        DhcpMessageType::Inform => "INFORM",
        DhcpMessageType::Other(_) => "OTHER",
    }
}

#[embassy_executor::task]
async fn dhcp_server_task(
    stack: embassy_net::Stack<'static>,
    server_ip: Ipv4Address,
    netmask: Ipv4Address,
    pool_start: Ipv4Address,
    pool_size: u8,
) -> ! {
    let mut rx_meta = [udp::PacketMetadata::EMPTY; 4];
    let mut rx_buffer = [0u8; 768];
    let mut tx_meta = [udp::PacketMetadata::EMPTY; 4];
    let mut tx_buffer = [0u8; 768];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    if let Err(err) = socket.bind(DHCP_SERVER_PORT) {
        error!("DHCP server failed to bind: {:?}", err);
        defmt::panic!("Unable to bind DHCP port");
    }

    let broadcast_ip = Ipv4Address::new(
        server_ip.octets()[0],
        server_ip.octets()[1],
        server_ip.octets()[2],
        255,
    );

    info!("DHCP server listening on {}", server_ip);

    let mut leases: heapless::Vec<DhcpLease, 8> = heapless::Vec::new();
    let mut frame = [0u8; 768];
    let mut response = [0u8; 768];

    loop {
        let recv = socket.recv_from(&mut frame).await;
        let (len, remote) = match recv {
            Ok(data) => data,
            Err(err) => {
                warn!("DHCP recv error: {:?}", err);
                continue;
            }
        };

        let Some(message) = parse_dhcp_message(&frame[..len]) else {
            trace!("Ignoring malformed DHCP packet from {:?}", remote);
            continue;
        };

        let label = message_kind_label(message.msg_type);
        debug!("DHCP {} from {}", label, Debug2Format(&message.client_mac));

        if matches!(message.msg_type, DhcpMessageType::Request)
            && message.server_id.is_some()
            && message.server_id != Some(server_ip)
        {
            trace!("DHCP REQUEST for different server, ignoring");
            continue;
        }

        let offer_ip = match message.msg_type {
            DhcpMessageType::Discover | DhcpMessageType::Request => ensure_lease(
                &mut leases,
                message.client_mac,
                pool_start,
                pool_size,
                message.requested_ip.or(message.client_ip),
            )
            .unwrap_or(pool_start),
            DhcpMessageType::Decline | DhcpMessageType::Release => {
                leases.retain(|lease| lease.mac != message.client_mac);
                continue;
            }
            DhcpMessageType::Inform | DhcpMessageType::Other(_) => continue,
        };

        let response_kind = match message.msg_type {
            DhcpMessageType::Discover => DhcpMessageType::Discover,
            DhcpMessageType::Request => DhcpMessageType::Request,
            _ => message.msg_type,
        };

        let Some(response_len) = build_dhcp_reply(
            &mut response,
            &message,
            offer_ip,
            server_ip,
            netmask,
            broadcast_ip,
            response_kind,
        ) else {
            warn!("Failed to build DHCP response");
            continue;
        };

        let response_label = match response_kind {
            DhcpMessageType::Discover => "OFFER",
            DhcpMessageType::Request => "ACK",
            DhcpMessageType::Decline => "DECLINE",
            DhcpMessageType::Release => "RELEASE",
            DhcpMessageType::Inform => "INFORM",
            DhcpMessageType::Other(_) => "OTHER",
        };

        if let Err(err) = socket
            .send_to(
                &response[..response_len],
                (IpAddress::Ipv4(Ipv4Address::BROADCAST), DHCP_CLIENT_PORT),
            )
            .await
        {
            warn!("Failed to send DHCP response: {:?}", err);
        } else {
            info!("Sent DHCP {} for {}", response_label, offer_ip);
        }
    }
}

#[embassy_executor::task]
async fn dns_server_task(stack: embassy_net::Stack<'static>, answer_ip: Ipv4Address) -> ! {
    let mut rx_meta = [udp::PacketMetadata::EMPTY; 4];
    let mut rx_buffer = [0u8; 768];
    let mut tx_meta = [udp::PacketMetadata::EMPTY; 4];
    let mut tx_buffer = [0u8; 768];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    if let Err(err) = socket.bind(DNS_SERVER_PORT) {
        error!("DNS server failed to bind: {:?}", err);
        defmt::panic!("Unable to bind DNS port");
    }

    info!("DNS server responding with {}", answer_ip);

    let mut frame = [0u8; 512];
    let mut response = [0u8; 512];

    loop {
        let Ok((len, remote)) = socket.recv_from(&mut frame).await else {
            continue;
        };

        let query = &frame[..len];
        let Some(question) = parse_dns_question(query) else {
            trace!("Ignoring malformed DNS query");
            continue;
        };

        let Some(resp_len) = build_dns_response(query, &mut response, answer_ip, &question) else {
            trace!("Failed to build DNS response");
            continue;
        };

        if let Err(err) = socket.send_to(&response[..resp_len], remote).await {
            warn!("DNS send error: {:?}", err);
            continue;
        }

        let name = if question.name.is_empty() {
            "(root)"
        } else {
            question.name.as_str()
        };
        info!(
            "DNS {} -> {} (qtype {})",
            name,
            answer_ip,
            question.qtype
        );
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    info!("Starting WiFi access-point web server example");

    let p = embassy_rp::init(Default::default());

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
    spawner.spawn(defmt::unwrap!(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let ap_ip = Ipv4Address::new(192, 168, 4, 1);
    let network = Ipv4Cidr::new(ap_ip, 24);
    let netmask = network.netmask();
    let pool_start = bump_ipv4(ap_ip, 1);
    let pool_size: u8 = 8;

    let static_cfg = embassy_net::StaticConfigV4 {
        address: network,
        gateway: None,
        dns_servers: heapless::Vec::new(),
    };
    let config = Config::ipv4_static(static_cfg);

    static NET_RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
    let (stack, net_runner) = embassy_net::new(
        net_device,
        config,
        NET_RESOURCES.init(StackResources::new()),
        0x1357_2468_9abc_def0,
    );
    spawner.spawn(defmt::unwrap!(net_task(net_runner)));
    spawner.spawn(defmt::unwrap!(dhcp_server_task(
        stack, ap_ip, netmask, pool_start, pool_size,
    )));
    spawner.spawn(defmt::unwrap!(dns_server_task(stack, ap_ip)));

    let ssid = ap_ssid();
    let password = ap_password();
    let channel = ap_channel();

    if password.is_empty() {
        info!(
            "Starting OPEN access point \"{}\" on channel {}",
            ssid, channel
        );
        control.start_ap_open(ssid, channel).await;
    } else {
        if !(8..=63).contains(&password.len()) {
            error!(
                "PICO_AP_PASSWORD must be 8-63 characters long (got {})",
                password.len()
            );
            defmt::panic!("Invalid AP password length");
        }
        info!(
            "Starting WPA2 access point \"{}\" on channel {}",
            ssid, channel
        );
        control.start_ap_wpa2(ssid, password, channel).await;
    }

    info!("Access point ready — connect to SSID \"{}\"", ssid);
    if password.is_empty() {
        info!("Network is open (no password)");
    } else {
        info!("Password: {}", password);
    }
    info!("Device IP address: {}", ap_ip);
    info!(
        "Configure a client with static IP (e.g. 192.168.4.2/24, gateway {})",
        ap_ip
    );

    let mut rx_buffer = [0u8; 2048];
    let mut tx_buffer = [0u8; 4096];
    let mut request = [0u8; 1024];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(30)));

        info!("Listening for HTTP clients on port 80...");
        if let Err(err) = socket.accept(80).await {
            warn!("accept error: {:?}", err);
            Timer::after_millis(500).await;
            continue;
        }

        info!("Client connected: {:?}", socket.remote_endpoint());

        let request_len = match socket.read(&mut request).await {
            Ok(0) => {
                info!("Client closed without sending data");
                let _ = socket.flush().await;
                socket.close();
                continue;
            }
            Ok(n) => {
                let preview = from_utf8(&request[..n]).unwrap_or("<non-UTF8>");
                info!("Request preview: {}", preview);
                n
            }
            Err(err) => {
                warn!("Read error: {:?}", err);
                let _ = socket.flush().await;
                socket.close();
                continue;
            }
        };

        let request_text = from_utf8(&request[..request_len]).unwrap_or("");
        let mut lines = request_text.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let raw_path = parts.next().unwrap_or("/");
        let cleaned_path = raw_path.split('?').next().unwrap_or("/");
        let path = if cleaned_path.starts_with('/') {
            cleaned_path
        } else {
            "/"
        };

        let mut headers = String::<512>::new();
        let status_line;
        let mut content_type = "text/plain; charset=utf-8";
        let body: &[u8];
        let mut send_body = true;
        let mut location: Option<&str> = None;
        let mut cache_control: Option<&str> = None;
        let mut _is_captive_probe = false;

        if method.is_empty() {
            status_line = "HTTP/1.1 400 Bad Request\r\n";
            body = b"Bad Request";
        } else {
            match method {
                "GET" | "HEAD" => {
                    send_body = method == "GET";
                    if let Some(captive) =
                        CAPTIVE_PORTAL_RESPONSES.iter().find(|resp| resp.path == path)
                    {
                        status_line = captive.status_line;
                        content_type = captive.content_type;
                        body = captive.body;
                        location = captive.location;
                        cache_control = Some("no-store, max-age=0");
                        _is_captive_probe = true;
                        info!(
                            "Captive portal response {} {}",
                            path,
                            captive.status_line.trim()
                        );
                    } else if let Some(asset) = find_asset(path) {
                        status_line = "HTTP/1.1 200 OK\r\n";
                        content_type = asset.content_type;
                        body = asset.body;
                        cache_control = Some("no-store, max-age=0");
                        if send_body {
                            info!("Serving asset {} ({} bytes)", asset.path, asset.body.len());
                        } else {
                            info!("HEAD {}", asset.path);
                        }
                    } else {
                        status_line = "HTTP/1.1 404 Not Found\r\n";
                        body = b"Not Found";
                        cache_control = Some("no-store, max-age=0");
                    }
                }
                _ => {
                    status_line = "HTTP/1.1 405 Method Not Allowed\r\n";
                    body = b"Method Not Allowed";
                }
            }
        }

        headers.push_str(status_line).unwrap();
        write!(&mut headers, "Content-Type: {}\r\n", content_type).unwrap();
        write!(&mut headers, "Content-Length: {}\r\n", body.len()).unwrap();
        if let Some(loc) = location {
            write!(&mut headers, "Location: {}\r\n", loc).unwrap();
        }
        if let Some(cache) = cache_control {
            write!(&mut headers, "Cache-Control: {}\r\n", cache).unwrap();
        }
        
        // Note: COOP/COEP headers are NOT added because http://192.168.4.1 is not a secure context.
        // Cross-origin isolation requires HTTPS (or localhost). The JS code detects this and uses
        // inline WASM execution instead of workers that need SharedArrayBuffer.
        
        if status_line.contains("405") {
            headers.push_str("Allow: GET, HEAD\r\n").unwrap();
        }
        headers.push_str("Connection: close\r\n\r\n").unwrap();

        if let Err(err) = socket.write_all(headers.as_bytes()).await {
            warn!("Failed to write headers: {:?}", err);
            socket.abort();
            continue;
        }
        if send_body {
            if let Err(err) = socket.write_all(body).await {
                warn!("Failed to write body: {:?}", err);
                socket.abort();
                continue;
            }
        }

        let _ = socket.flush().await;
        socket.close();

        info!("Response sent; connection closed");
        Timer::after_millis(200).await;
    }
}
