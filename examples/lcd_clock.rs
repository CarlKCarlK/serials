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
use heapless::String;
use lib::{CharLcd, LcdChannel};
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
    // Read configuration from compile-time environment (set by build.rs)
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");
    const TIMEZONE: &str = env!("TIMEZONE");

    info!("Starting LCD Clock...");
    info!("Timezone: {}", TIMEZONE);

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Initialize LCD (GP4=SDA, GP5=SCL)
    static LCD_CHANNEL: LcdChannel = CharLcd::channel();
    let lcd = match CharLcd::new(p.I2C0, p.PIN_5, p.PIN_4, &LCD_CHANNEL, spawner) {
        Ok(lcd) => lcd,
        Err(_) => core::panic!("LCD init failed"),
    };
    lcd.display(String::<64>::try_from("Connecting WiFi").unwrap(), 0);

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

    // Generate random seed
    let seed = 0x0123_4567_89ab_cdef;

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
    info!("Connecting to WiFi: {}", WIFI_SSID);
    loop {
        match control.join(WIFI_SSID, JoinOptions::new(WIFI_PASS.as_bytes())).await {
            Ok(_) => break,
            Err(err) => {
                info!("Join failed: {}", err.status);
                Timer::after_secs(1).await;
            }
        }
    }

    lcd.display(String::<64>::try_from("WiFi Connected!").unwrap(), 1000);
    Timer::after_secs(1).await;

    info!("WiFi connected! Waiting for DHCP...");
    stack.wait_config_up().await;

    lcd.display(String::<64>::try_from("Getting time...").unwrap(), 0);

    if let Some(config) = stack.config_v4() {
        info!("IP Address: {}", config.address);
    }

    // Fetch initial time from internet
    let mut current_time = 'outer: loop {
        for attempt in 1..=3 {
            match fetch_local_time(&stack, TIMEZONE).await {
                Ok((hour, minute, second, date)) => {
                    info!("Initial sync: {}:{}:{} | {}", hour, minute, second, date);
                    break 'outer (hour, minute, second, date);
                }
                Err(e) => {
                    info!("Time fetch attempt {} failed: {}", attempt, e);
                    if attempt < 3 {
                        lcd.display(String::<64>::try_from("Retrying...").unwrap(), 0);
                        // Exponential backoff: 5s, 10s, 20s
                        let delay_secs = 5_u64 << (attempt - 1);
                        Timer::after_secs(delay_secs).await;
                    } else {
                        lcd.display(String::<64>::try_from("Sync failed!\nWaiting 60s...").unwrap(), 0);
                        Timer::after_secs(60).await; // Wait a full minute before trying again
                    }
                }
            }
        }
    };

    let mut seconds_since_sync = 0;

    // Main loop: keep time locally, sync every hour
    loop {
        // Display current time on LCD (two lines: time on top, date on bottom)
        let (hour, minute, second, date) = current_time;
        let (hour12, am_pm) = if hour == 0 {
            (12, "AM")
        } else if hour < 12 {
            (hour, "AM")
        } else if hour == 12 {
            (12, "PM")
        } else {
            #[expect(clippy::arithmetic_side_effects, reason = "hour guaranteed 13-23")]
            (hour - 12, "PM")
        };
        
        let mut text = String::<64>::new();
        core::fmt::Write::write_fmt(&mut text, format_args!("{:2}:{:02}:{:02} {}\n{}", 
            hour12, minute, second, am_pm, date)).unwrap();
        lcd.display(text, 0);
        
        // Wait one second
        Timer::after_secs(1).await;
        
        // Increment time by one second
        #[expect(clippy::arithmetic_side_effects, reason = "time arithmetic with wrapping")]
        {
            current_time.2 += 1; // Increment second
            if current_time.2 >= 60 {
                current_time.2 = 0;
                current_time.1 += 1; // Increment minute
                if current_time.1 >= 60 {
                    current_time.1 = 0;
                    current_time.0 += 1; // Increment hour
                    if current_time.0 >= 24 {
                        current_time.0 = 0;
                        // Note: We don't handle date rollover - will re-sync before that matters
                    }
                }
            }
        }
        
        seconds_since_sync += 1;
        
        // Sync with internet every 3600 seconds (60 minutes)
        if seconds_since_sync >= 3600 {
            info!("Hourly sync...");
            lcd.display(String::<64>::try_from("Syncing time...").unwrap(), 0);
            
            // Only try once for hourly sync to avoid rate limiting
            match fetch_local_time(&stack, TIMEZONE).await {
                Ok(new_time) => {
                    current_time = new_time;
                    seconds_since_sync = 0;
                    info!("Sync successful: {}:{}:{}", new_time.0, new_time.1, new_time.2);
                }
                Err(e) => {
                    info!("Hourly sync failed: {}", e);
                    // Keep using local time if sync fails
                    seconds_since_sync = 0; // Reset to try again in an hour
                }
            }
        }
    }
}

async fn fetch_local_time(
    stack: &embassy_net::Stack<'static>,
    timezone: &str,
) -> Result<(u8, u8, u8, &'static str), &'static str> {
    use embassy_net::tcp::TcpSocket;
    use embassy_net::dns::DnsQueryType;
    use heapless::String;
    use embedded_io_async::Write;
    
    // DNS lookup for worldtimeapi.org
    info!("Resolving worldtimeapi.org...");
    let dns_result = stack
        .dns_query("worldtimeapi.org", DnsQueryType::A)
        .await
        .map_err(|_| "DNS lookup failed")?;
    let server_addr = dns_result
        .first()
        .ok_or("No DNS results")?;
    
    info!("Server IP: {}", server_addr);
    
    // Create TCP socket for HTTP request
    let mut tcp_rx_buffer = [0; 1024];
    let mut tcp_tx_buffer = [0; 512];
    let mut socket = TcpSocket::new(*stack, &mut tcp_rx_buffer, &mut tcp_tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));
    
    // Connect to server on port 80
    let remote_endpoint = (*server_addr, 80);
    info!("Connecting to {}:80...", server_addr);
    socket.connect(remote_endpoint).await.map_err(|_| "Connect failed")?;
    
    // Small delay after connect to let connection stabilize
    Timer::after_millis(100).await;
    
    // Build HTTP GET request
    let mut request = String::<256>::new();
    core::fmt::write(&mut request, format_args!("GET /api/timezone/{} HTTP/1.1\r\n", timezone)).unwrap();
    core::fmt::write(&mut request, format_args!("Host: worldtimeapi.org\r\n")).unwrap();
    core::fmt::write(&mut request, format_args!("Connection: close\r\n")).unwrap();
    core::fmt::write(&mut request, format_args!("\r\n")).unwrap();
    
    info!("Sending HTTP request...");
    Write::write_all(&mut socket, request.as_bytes()).await.map_err(|_| "Write failed")?;
    
    // Small delay after write to let server process
    Timer::after_millis(100).await;
    
    // Read response
    let mut response = [0u8; 1024];
    let mut total_read = 0;
    
    loop {
        match socket.read(&mut response[total_read..]).await {
            Ok(0) => break,
            Ok(n) => {
                total_read += n;
                #[expect(clippy::arithmetic_side_effects, reason = "bounded by response.len()")]
                if total_read >= response.len() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    
    socket.close();
    
    if total_read == 0 {
        return Err("No response from server");
    }
    
    // Parse datetime from JSON response
    let response_str = core::str::from_utf8(&response[..total_read]).map_err(|_| "Invalid UTF-8")?;
    
    // Find datetime field in JSON: "datetime":"2025-10-19T14:30:00.123456-07:00"
    if let Some(datetime_start) = response_str.find("\"datetime\":\"") {
        #[expect(clippy::arithmetic_side_effects, reason = "string index arithmetic")]
        let value_start = datetime_start + "\"datetime\":\"".len();
        if let Some(value_end) = response_str[value_start..].find('"') {
            #[expect(clippy::arithmetic_side_effects, reason = "string index arithmetic")]
            let datetime = &response_str[value_start..value_start + value_end];
            
            // Parse datetime: "2025-10-19T14:30:00.123456-07:00"
            if datetime.len() >= 19 {
                #[expect(clippy::indexing_slicing, reason = "datetime format verified")]
                let hour24: u8 = datetime[11..13].parse().map_err(|_| "Invalid hour")?;
                #[expect(clippy::indexing_slicing, reason = "datetime format verified")]
                let minute: u8 = datetime[14..16].parse().map_err(|_| "Invalid minute")?;
                #[expect(clippy::indexing_slicing, reason = "datetime format verified")]
                let second: u8 = datetime[17..19].parse().map_err(|_| "Invalid second")?;
                #[expect(clippy::indexing_slicing, reason = "datetime format verified")]
                let date = &datetime[..10]; // "2025-10-19"
                
                // Store date in static for return
                static mut DATE_STR: [u8; 16] = [0; 16];
                
                #[expect(unsafe_code, reason = "static string storage for return value")]
                unsafe {
                    let mut date_result = String::<16>::new();
                    core::fmt::write(&mut date_result, format_args!("{}", date)).unwrap();
                    DATE_STR[..date_result.len()].copy_from_slice(date_result.as_bytes());
                    
                    Ok((
                        hour24,
                        minute,
                        second,
                        core::str::from_utf8_unchecked(&DATE_STR[..date_result.len()]),
                    ))
                }
            } else {
                Err("Invalid datetime format")
            }
        } else {
            Err("Datetime value not terminated")
        }
    } else {
        Err("Datetime not found in response")
    }
}
