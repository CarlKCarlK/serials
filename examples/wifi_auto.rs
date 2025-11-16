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

use core::{cell::RefCell, convert::Infallible, fmt::Write as FmtWrite};
use defmt::{info, warn};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_net::{Stack, dns::DnsQueryType, udp};
use embassy_rp::gpio::{self, Level};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_time::Duration;
use heapless::String;
use panic_probe as _;
use serials::flash_array::{FlashArray, FlashArrayNotifier, FlashBlock};
use serials::led4::{AnimationFrame, BlinkState, Led4, Led4Animation, Led4Notifier, OutputArray};
use serials::unix_seconds::UnixSeconds;
use serials::wifi_auto::{
    FormData, HtmlBuffer, WifiAuto, WifiAutoConfig, WifiAutoEvent, WifiAutoField, WifiAutoNotifier,
};
use serials::{Error, Result};
use static_cell::StaticCell;

type UserName = String<32>;

static TIMEZONE_FIELD_CELL: StaticCell<TimezoneField> = StaticCell::new();
static USER_NAME_FIELD_CELL: StaticCell<UserNameField> = StaticCell::new();
static FIELD_COLLECTION: StaticCell<[&'static dyn WifiAutoField; 2]> = StaticCell::new();

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

    let timezone_field = TIMEZONE_FIELD_CELL.init(TimezoneField::new(timezone_flash));
    let user_name_field =
        USER_NAME_FIELD_CELL.init(UserNameField::new(nickname_flash, "PicoClock", 32));
    let field_slice = FIELD_COLLECTION.init([
        timezone_field as &dyn WifiAutoField,
        user_name_field as &dyn WifiAutoField,
    ]);

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

    static WIFI_AUTO_NOTIFIER: WifiAutoNotifier = WifiAuto::notifier();
    let wifi_auto = WifiAuto::new(
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
        WifiAutoConfig::new().with_fields(field_slice),
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

    let timezone_offset_minutes = timezone_field.load_offset()?.unwrap_or(0);
    let nickname = user_name_field
        .load_name()?
        .unwrap_or_else(|| user_name_field.default_name());
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

struct TimezoneField {
    flash: Mutex<CriticalSectionRawMutex, RefCell<FlashBlock>>,
}

impl TimezoneField {
    fn new(flash: FlashBlock) -> Self {
        Self {
            flash: Mutex::new(RefCell::new(flash)),
        }
    }

    fn load_offset(&self) -> Result<Option<i32>> {
        self.flash.lock(|cell| cell.borrow_mut().load::<i32>())
    }

    fn save_offset(&self, offset: i32) -> Result<()> {
        self.flash.lock(|cell| cell.borrow_mut().save(&offset))
    }
}

impl WifiAutoField for TimezoneField {
    fn render(&self, page: &mut HtmlBuffer) -> Result<()> {
        info!("WifiAuto field: rendering timezone select");
        let current = self.load_offset()?.unwrap_or(0);
        FmtWrite::write_str(page, "<label for=\"timezone\">Time zone:</label>")
            .map_err(|_| Error::FormatError)?;
        FmtWrite::write_str(page, "<select id=\"timezone\" name=\"timezone\" required>")
            .map_err(|_| Error::FormatError)?;
        for option in TIMEZONE_OPTIONS {
            let selected = if option.minutes == current {
                " selected"
            } else {
                ""
            };
            write!(
                page,
                "<option value=\"{}\"{}>{}</option>",
                option.minutes, selected, option.label
            )
            .map_err(|_| Error::FormatError)?;
        }
        page.push_str("</select>").map_err(|_| Error::FormatError)?;
        Ok(())
    }

    fn parse(&self, form: &FormData<'_>) -> Result<()> {
        let value = form.get("timezone").ok_or(Error::FormatError)?;
        let offset = value.parse::<i32>().map_err(|_| Error::FormatError)?;
        self.save_offset(offset)
    }

    fn is_satisfied(&self) -> Result<bool> {
        Ok(self.load_offset()?.is_some())
    }
}

struct TimezoneOption {
    minutes: i32,
    label: &'static str,
}

const TIMEZONE_OPTIONS: &[TimezoneOption] = &[
    TimezoneOption {
        minutes: -720,
        label: "Baker Island (UTC-12:00)",
    },
    TimezoneOption {
        minutes: -660,
        label: "American Samoa (UTC-11:00)",
    },
    TimezoneOption {
        minutes: -600,
        label: "Honolulu (UTC-10:00)",
    },
    TimezoneOption {
        minutes: -540,
        label: "Anchorage, Alaska ST (UTC-09:00)",
    },
    TimezoneOption {
        minutes: -480,
        label: "Anchorage, Alaska DT (UTC-08:00)",
    },
    TimezoneOption {
        minutes: -480,
        label: "Los Angeles, San Francisco, Seattle ST (UTC-08:00)",
    },
    TimezoneOption {
        minutes: -420,
        label: "Los Angeles, San Francisco, Seattle DT (UTC-07:00)",
    },
    TimezoneOption {
        minutes: -420,
        label: "Denver, Phoenix ST (UTC-07:00)",
    },
    TimezoneOption {
        minutes: -360,
        label: "Denver DT (UTC-06:00)",
    },
    TimezoneOption {
        minutes: -360,
        label: "Chicago, Dallas, Mexico City ST (UTC-06:00)",
    },
    TimezoneOption {
        minutes: -300,
        label: "Chicago, Dallas DT (UTC-05:00)",
    },
    TimezoneOption {
        minutes: -300,
        label: "New York, Toronto, Bogota ST (UTC-05:00)",
    },
    TimezoneOption {
        minutes: -240,
        label: "New York, Toronto DT (UTC-04:00)",
    },
    TimezoneOption {
        minutes: -240,
        label: "Santiago, Halifax ST (UTC-04:00)",
    },
    TimezoneOption {
        minutes: -210,
        label: "St. John's, Newfoundland ST (UTC-03:30)",
    },
    TimezoneOption {
        minutes: -180,
        label: "Buenos Aires, Sao Paulo (UTC-03:00)",
    },
    TimezoneOption {
        minutes: -120,
        label: "South Georgia (UTC-02:00)",
    },
    TimezoneOption {
        minutes: -60,
        label: "Azores ST (UTC-01:00)",
    },
    TimezoneOption {
        minutes: 0,
        label: "London, Lisbon ST (UTC+00:00)",
    },
    TimezoneOption {
        minutes: 60,
        label: "London, Paris, Berlin DT (UTC+01:00)",
    },
    TimezoneOption {
        minutes: 60,
        label: "Paris, Berlin, Rome ST (UTC+01:00)",
    },
    TimezoneOption {
        minutes: 120,
        label: "Paris, Berlin, Rome DT (UTC+02:00)",
    },
    TimezoneOption {
        minutes: 120,
        label: "Athens, Cairo, Johannesburg ST (UTC+02:00)",
    },
    TimezoneOption {
        minutes: 180,
        label: "Athens DT (UTC+03:00)",
    },
    TimezoneOption {
        minutes: 180,
        label: "Moscow, Istanbul, Nairobi (UTC+03:00)",
    },
    TimezoneOption {
        minutes: 240,
        label: "Dubai, Baku (UTC+04:00)",
    },
    TimezoneOption {
        minutes: 270,
        label: "Tehran ST (UTC+04:30)",
    },
    TimezoneOption {
        minutes: 300,
        label: "Karachi, Tashkent (UTC+05:00)",
    },
    TimezoneOption {
        minutes: 330,
        label: "Mumbai, Delhi (UTC+05:30)",
    },
    TimezoneOption {
        minutes: 345,
        label: "Kathmandu (UTC+05:45)",
    },
    TimezoneOption {
        minutes: 360,
        label: "Dhaka, Almaty (UTC+06:00)",
    },
    TimezoneOption {
        minutes: 390,
        label: "Yangon (UTC+06:30)",
    },
    TimezoneOption {
        minutes: 420,
        label: "Bangkok, Jakarta (UTC+07:00)",
    },
    TimezoneOption {
        minutes: 480,
        label: "Singapore, Hong Kong, Beijing (UTC+08:00)",
    },
    TimezoneOption {
        minutes: 525,
        label: "Eucla, Australia (UTC+08:45)",
    },
    TimezoneOption {
        minutes: 540,
        label: "Tokyo, Seoul (UTC+09:00)",
    },
    TimezoneOption {
        minutes: 570,
        label: "Adelaide ST (UTC+09:30)",
    },
    TimezoneOption {
        minutes: 600,
        label: "Sydney, Melbourne ST (UTC+10:00)",
    },
    TimezoneOption {
        minutes: 630,
        label: "Adelaide DT (UTC+10:30)",
    },
    TimezoneOption {
        minutes: 660,
        label: "Sydney, Melbourne DT (UTC+11:00)",
    },
    TimezoneOption {
        minutes: 720,
        label: "Auckland, Fiji ST (UTC+12:00)",
    },
    TimezoneOption {
        minutes: 780,
        label: "Auckland DT (UTC+13:00)",
    },
    TimezoneOption {
        minutes: 840,
        label: "Kiribati (UTC+14:00)",
    },
];

struct UserNameField {
    flash: Mutex<CriticalSectionRawMutex, RefCell<FlashBlock>>,
    max_len: usize,
    placeholder: &'static str,
}

impl UserNameField {
    fn new(flash: FlashBlock, placeholder: &'static str, max_len: usize) -> Self {
        Self {
            flash: Mutex::new(RefCell::new(flash)),
            max_len,
            placeholder,
        }
    }

    fn load_name(&self) -> Result<Option<UserName>> {
        self.flash.lock(|cell| cell.borrow_mut().load::<UserName>())
    }

    fn save_name(&self, name: &UserName) -> Result<()> {
        self.flash.lock(|cell| cell.borrow_mut().save(name))
    }

    fn default_name(&self) -> UserName {
        let mut name = UserName::new();
        let _ = name.push_str(self.placeholder);
        name
    }
}

impl WifiAutoField for UserNameField {
    fn render(&self, page: &mut HtmlBuffer) -> Result<()> {
        info!("WifiAuto field: rendering user name input");
        let current = self
            .load_name()?
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.default_name());
        let escaped = simple_escape(current.as_str());
        write!(
            page,
            "<label for=\"nickname\">User name:</label>\
             <input type=\"text\" id=\"nickname\" name=\"nickname\" value=\"{}\" \
             maxlength=\"{}\" required>",
            escaped, self.max_len
        )
        .map_err(|_| Error::FormatError)?;
        Ok(())
    }

    fn parse(&self, form: &FormData<'_>) -> Result<()> {
        let Some(value) = form.get("nickname") else {
            info!("WifiAuto field: user name missing from submission");
            return Ok(());
        };
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > self.max_len {
            return Err(Error::FormatError);
        }
        let mut name = UserName::new();
        name.push_str(trimmed).map_err(|_| Error::FormatError)?;
        self.save_name(&name)
    }

    fn is_satisfied(&self) -> Result<bool> {
        Ok(self.load_name()?.map_or(false, |name| !name.is_empty()))
    }
}

fn simple_escape(input: &str) -> String<128> {
    let mut escaped = String::<128>::new();
    for ch in input.chars() {
        match ch {
            '&' => {
                let _ = escaped.push_str("&amp;");
            }
            '<' => {
                let _ = escaped.push_str("&lt;");
            }
            '>' => {
                let _ = escaped.push_str("&gt;");
            }
            '"' => {
                let _ = escaped.push_str("&quot;");
            }
            '\'' => {
                let _ = escaped.push_str("&#39;");
            }
            _ => {
                let _ = escaped.push(ch);
            }
        }
    }
    escaped
}
