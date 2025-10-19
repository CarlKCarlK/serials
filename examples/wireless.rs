#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_net::{Config, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::{Duration, Timer};
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

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

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    // Read Wi-Fi credentials from compile-time environment (set by build.rs)
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");

    info!("Starting Pico W wireless example...");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

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

    // Generate random seed (using timer as entropy source)
    let seed = 0x0123_4567_89ab_cdef; // In production, use better entropy

    // Init network stack
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<3>::new()),
        seed,
    );

    unwrap!(spawner.spawn(net_task(runner)));

    // Connect to WiFi
    info!("Connecting to WiFi SSID: {}", WIFI_SSID);
    loop {
        match control.join(WIFI_SSID, JoinOptions::new(WIFI_PASS.as_bytes())).await {
            Ok(_) => break,
            Err(err) => {
                info!("Join failed with status={}", err.status);
                Timer::after_secs(1).await;
            }
        }
    }

    info!("WiFi connected! Waiting for DHCP...");
    stack.wait_config_up().await;
    info!("DHCP configured!");

    if let Some(config) = stack.config_v4() {
        info!("IP Address: {}", config.address);
    }

    // Simple NTP time fetch using UDP
    info!("Fetching time from NTP server...");
    
    // NTP server: pool.ntp.org (or use 216.239.35.0 for time.google.com)
    let ntp_server = embassy_net::Ipv4Address::new(216, 239, 35, 0);
    
    loop {
        match fetch_ntp_time(&stack, ntp_server).await {
            Ok(timestamp) => {
                info!("NTP Timestamp: {} seconds since 1900-01-01", timestamp);
                
                // Convert NTP timestamp to Unix timestamp (seconds since 1970-01-01)
                // NTP epoch is 1900-01-01, Unix epoch is 1970-01-01
                // Difference: 2208988800 seconds (70 years)
                #[expect(clippy::arithmetic_side_effects, reason = "NTP constant conversion")]
                let unix_timestamp = timestamp - 2_208_988_800;
                
                info!("Unix Timestamp: {} seconds since 1970-01-01", unix_timestamp);
                
                // Simple date/time calculation
                let seconds = unix_timestamp % 60;
                let minutes = (unix_timestamp / 60) % 60;
                let hours = (unix_timestamp / 3600) % 24;
                
                info!("Current UTC Time: {:02}:{:02}:{:02}", hours, minutes, seconds);
            }
            Err(e) => {
                info!("NTP fetch failed: {}", e);
            }
        }
        
        Timer::after_secs(10).await;
    }
}

async fn fetch_ntp_time(
    stack: &embassy_net::Stack<'static>,
    server: embassy_net::Ipv4Address,
) -> Result<u32, &'static str> {
    use embassy_net::udp::{PacketMetadata, UdpSocket};
    
    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buffer = [0; 128];  // Larger buffer for socket
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_buffer = [0; 48];
    
    let mut socket = UdpSocket::new(
        *stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );
    
    socket.bind(123).map_err(|_| "Bind failed")?;
    
    // NTP request packet (48 bytes, mode 3 = client)
    let mut ntp_request = [0u8; 48];
    ntp_request[0] = 0x1B; // LI=0, VN=3, Mode=3
    
    // Send NTP request
    socket
        .send_to(&ntp_request, (server, 123))
        .await
        .map_err(|_| "Send failed")?;
    
    // Receive NTP response with timeout
    let mut ntp_response = [0u8; 48];
    let (len, _addr) = embassy_time::with_timeout(
        Duration::from_secs(5), 
        socket.recv_from(&mut ntp_response)
    )
        .await
        .map_err(|_| "Timeout")?
        .map_err(|_| "Recv failed")?;
    
    if len < 48 {
        return Err("Invalid NTP response");
    }
    
    // Extract transmit timestamp (bytes 40-43)
    #[expect(clippy::indexing_slicing, reason = "NTP packet format verified")]
    let timestamp = u32::from_be_bytes([
        ntp_response[40],
        ntp_response[41],
        ntp_response[42],
        ntp_response[43],
    ]);
    
    Ok(timestamp)
}
