//! Simple DHCP server for captive portal mode
//!
//! Provides IP address leases to clients connecting to the WiFi captive portal.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::*;
use embassy_net::{
    Ipv4Address, Stack,
    udp::{self, UdpSocket},
};
use embassy_time::{Duration, Instant};

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
const DHCP_LEASE_SECONDS: u32 = 30; // Short lease keeps captive portal clients refreshing quickly

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
        // Only support Ethernet clients with 6-byte MACs
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

    let ip = desired_ip.or_else(|| {
        for offset in 0..pool_size {
            let base_u32 = u32::from_be_bytes(pool_start.octets());
            let candidate_u32 = base_u32.saturating_add(offset as u32);
            let octets = candidate_u32.to_be_bytes();
            let candidate = Ipv4Address::new(octets[0], octets[1], octets[2], octets[3]);

            if leases.iter().all(|lease| lease.ip != candidate) {
                return Some(candidate);
            }
        }
        None
    })?;

    leases
        .push(DhcpLease {
            mac,
            ip,
            expires_at: expiry,
        })
        .ok()?;

    Some(ip)
}

#[embassy_executor::task]
pub async fn dhcp_server_task(
    stack: &'static Stack<'static>,
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
        *stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    if let Err(err) = socket.bind(DHCP_SERVER_PORT) {
        error!("DHCP server failed to bind: {:?}", err);
        core::panic!("Unable to bind DHCP port");
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
        let (len, _remote) = match recv {
            Ok(data) => data,
            Err(err) => {
                warn!("DHCP recv error: {:?}", err);
                continue;
            }
        };

        let Some(message) = parse_dhcp_message(&frame[..len]) else {
            continue;
        };

        debug!(
            "DHCP {:?} from {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            message.msg_type,
            message.client_mac[0],
            message.client_mac[1],
            message.client_mac[2],
            message.client_mac[3],
            message.client_mac[4],
            message.client_mac[5]
        );

        if matches!(message.msg_type, DhcpMessageType::Request)
            && message.server_id.is_some()
            && message.server_id != Some(server_ip)
        {
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
            warn!("Failed to build DHCP reply");
            continue;
        };

        let broadcast_addr = (broadcast_ip, 68);
        if let Err(err) = socket
            .send_to(&response[..response_len], broadcast_addr)
            .await
        {
            warn!("DHCP send error: {:?}", err);
        } else {
            debug!("DHCP offered {} to client", offer_ip);
        }
    }
}
