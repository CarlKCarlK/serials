//! Simple DNS server for captive portal
//!
//! Responds to all DNS queries with the AP's IP address to support captive portal detection

#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::*;
use embassy_net::{
    Ipv4Address, Stack,
    udp::{self, UdpSocket},
};

const DNS_SERVER_PORT: u16 = 53;

/// DNS server task - responds to all queries with the AP IP address
/// This helps with captive portal detection on phones
#[embassy_executor::task]
pub async fn dns_server_task(stack: &'static Stack<'static>, answer_ip: Ipv4Address) -> ! {
    let mut rx_meta = [udp::PacketMetadata::EMPTY; 4];
    let mut rx_buffer = [0u8; 512];
    let mut tx_meta = [udp::PacketMetadata::EMPTY; 4];
    let mut tx_buffer = [0u8; 512];
    let mut socket = UdpSocket::new(
        *stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    if let Err(err) = socket.bind(DNS_SERVER_PORT) {
        error!("DNS server failed to bind: {:?}", err);
        core::panic!("Unable to bind DNS port");
    }

    info!("DNS server started - responding with {}", answer_ip);

    let mut frame = [0u8; 512];

    loop {
        let Ok((len, remote)) = socket.recv_from(&mut frame).await else {
            continue;
        };

        if len < 12 {
            // Too short to be valid DNS query
            continue;
        }

        // Simple DNS response - just echo back the query with an answer
        let mut response = [0u8; 512];
        response[..len].copy_from_slice(&frame[..len]);

        // Set response flags (QR=1, AA=1)
        response[2] = 0x84;
        response[3] = 0x00;

        // Answer count = 1
        response[6] = 0x00;
        response[7] = 0x01;

        let mut pos = len;

        // Add answer record
        // NAME: pointer to question name
        if pos + 16 <= response.len() {
            response[pos] = 0xC0;
            response[pos + 1] = 0x0C;
            pos += 2;

            // TYPE: A (1)
            response[pos] = 0x00;
            response[pos + 1] = 0x01;
            pos += 2;

            // CLASS: IN (1)
            response[pos] = 0x00;
            response[pos + 1] = 0x01;
            pos += 2;

            // TTL: 60 seconds
            response[pos] = 0x00;
            response[pos + 1] = 0x00;
            response[pos + 2] = 0x00;
            response[pos + 3] = 0x3C;
            pos += 4;

            // RDLENGTH: 4 bytes
            response[pos] = 0x00;
            response[pos + 1] = 0x04;
            pos += 2;

            // RDATA: IP address
            let octets = answer_ip.octets();
            response[pos..pos + 4].copy_from_slice(&octets);
            pos += 4;

            if let Err(err) = socket.send_to(&response[..pos], remote).await {
                warn!("DNS send error: {:?}", err);
            } else {
                debug!("DNS query answered with {}", answer_ip);
            }
        }
    }
}
