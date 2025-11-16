//! WiFi credential collection via captive portal web interface.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::{cell::RefCell, fmt::Write};
use defmt::{info, unwrap, warn};
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write as _;

use crate::Result;

/// WiFi network credentials (SSID and password).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WifiCredentials {
    /// Network SSID (up to 32 characters).
    pub ssid: heapless::String<32>,
    /// Network password (up to 64 characters).
    pub password: heapless::String<64>,
}

/// WiFi credentials combined with timezone offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiCredentialSubmission {
    pub credentials: WifiCredentials,
    pub timezone_offset_minutes: i32,
    pub nickname: Option<Nickname>,
}

/// Configuration for the WiFi credential form.
#[derive(Clone, Debug)]
pub struct WifiConfigOptions<'a> {
    pub defaults: Option<&'a WifiCredentials>,
    pub timezone: Option<TimezoneFieldOptions>,
    pub nickname: Option<NicknameFieldOptions>,
}

impl<'a> WifiConfigOptions<'a> {
    #[must_use]
    pub const fn with_defaults(defaults: Option<&'a WifiCredentials>) -> Self {
        Self {
            defaults,
            timezone: None,
            nickname: None,
        }
    }

    #[must_use]
    pub fn with_timezone(mut self, timezone: TimezoneFieldOptions) -> Self {
        self.timezone = Some(timezone);
        self
    }

    #[must_use]
    pub fn with_nickname(mut self, nickname: NicknameFieldOptions) -> Self {
        self.nickname = Some(nickname);
        self
    }
}

impl Default for WifiConfigOptions<'_> {
    fn default() -> Self {
        Self::with_defaults(None)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TimezoneFieldOptions {
    pub default_offset_minutes: i32,
}

#[derive(Clone, Debug)]
pub struct NicknameFieldOptions {
    pub default_value: Option<Nickname>,
    pub max_len: usize,
}

pub const MAX_NICKNAME_LEN: usize = 32;
pub type Nickname = heapless::String<MAX_NICKNAME_LEN>;

static CREDENTIAL_CHANNEL: Channel<CriticalSectionRawMutex, WifiCredentialSubmission, 1> =
    Channel::new();

#[derive(Clone)]
struct FormState {
    defaults: Option<WifiCredentials>,
    timezone: Option<TimezoneFormState>,
    nickname: Option<NicknameFormState>,
}

#[derive(Clone, Copy)]
struct TimezoneFormState {
    default_offset_minutes: i32,
}

#[derive(Clone)]
struct NicknameFormState {
    value: Nickname,
    max_len: usize,
}

static FORM_STATE: Mutex<CriticalSectionRawMutex, RefCell<FormState>> =
    Mutex::new(RefCell::new(FormState {
        defaults: None,
        timezone: None,
        nickname: None,
    }));

/// Collect WiFi credentials from user via web interface.
pub async fn collect_wifi_credentials(
    stack: &'static Stack<'static>,
    spawner: embassy_executor::Spawner,
    options: WifiConfigOptions<'_>,
) -> Result<WifiCredentialSubmission> {
    info!("Starting credential collection...");

    FORM_STATE.lock(|state| {
        let mut state = state.borrow_mut();
        state.defaults = options.defaults.cloned();
        state.timezone = options.timezone.map(|tz| TimezoneFormState {
            default_offset_minutes: tz.default_offset_minutes,
        });
        state.nickname = options.nickname.clone().map(|nick| {
            let max_len = nick.max_len.min(MAX_NICKNAME_LEN);
            let mut value = Nickname::new();
            if let Some(default) = nick.default_value.as_ref() {
                for ch in default.chars() {
                    if value.len() >= max_len {
                        break;
                    }
                    if value.push(ch).is_err() {
                        break;
                    }
                }
            }
            NicknameFormState { value, max_len }
        });
    });

    let token = unwrap!(http_config_server_task(stack));
    spawner.spawn(token);
    info!("HTTP configuration task spawned");

    info!("Waiting for user to submit credentials via web interface...");
    let submission = CREDENTIAL_CHANNEL.receive().await;

    info!("Credentials received!");
    Ok(submission)
}

/// HTTP server task for WiFi configuration
#[embassy_executor::task]
#[allow(unsafe_code)]
pub async fn http_config_server_task(stack: &'static Stack<'static>) -> ! {
    info!("HTTP config server starting on port 80");

    static mut RX_BUFFER: [u8; 2048] = [0u8; 2048];
    static mut TX_BUFFER: [u8; 4096] = [0u8; 4096];
    static mut REQUEST_BUFFER: [u8; 1024] = [0u8; 1024];

    let rx_buffer = unsafe { &mut *core::ptr::addr_of_mut!(RX_BUFFER) };
    let tx_buffer = unsafe { &mut *core::ptr::addr_of_mut!(TX_BUFFER) };
    let request = unsafe { &mut *core::ptr::addr_of_mut!(REQUEST_BUFFER) };

    loop {
        let mut socket = TcpSocket::new(*stack, rx_buffer, tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(30)));

        info!("Waiting for HTTP connection...");
        if let Err(e) = socket.accept(80).await {
            warn!("Accept error: {:?}", e);
            Timer::after_millis(500).await;
            continue;
        }

        info!("Client connected: {:?}", socket.remote_endpoint());

        let request_len = match socket.read(request).await {
            Ok(0) => {
                info!("Client closed without sending data");
                let _ = socket.flush().await;
                socket.close();
                continue;
            }
            Ok(n) => n,
            Err(e) => {
                warn!("Read error: {:?}", e);
                let _ = socket.flush().await;
                socket.close();
                continue;
            }
        };

        let request_text = core::str::from_utf8(&request[..request_len]).unwrap_or("");
        info!("Got HTTP request ({} bytes)", request_len);

        let mut lines = request_text.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("/");

        let state_snapshot = FORM_STATE.lock(|state| state.borrow().clone());

        let response: heapless::String<4096> = match (method, path) {
            ("GET", "/") => generate_config_page(&state_snapshot),
            ("GET", "/favicon.ico") => empty_favicon_response(),
            ("POST", "/") => {
                if let Some(submission) = parse_credentials_from_post(request_text, &state_snapshot)
                {
                    info!("Received WiFi credentials:");
                    info!("  SSID: {}", submission.credentials.ssid);
                    info!("  Password: [hidden]");

                    CREDENTIAL_CHANNEL.send(submission).await;
                    static_page(generate_success_page())
                } else {
                    warn!("Failed to parse credentials");
                    static_page(generate_error_page())
                }
            }
            _ => redirect_to_root(),
        };

        if let Err(e) = socket.write_all(response.as_bytes()).await {
            warn!("Write error: {:?}", e);
        }

        let _ = socket.flush().await;
        socket.close();
        Timer::after_millis(100).await;
    }
}

fn parse_credentials_from_post(
    request: &str,
    state: &FormState,
) -> Option<WifiCredentialSubmission> {
    let body_start = request.find("\r\n\r\n")? + 4;
    let body = &request[body_start..];

    info!("POST body: {}", body);

    let mut ssid = heapless::String::<32>::new();
    let mut password = heapless::String::<64>::new();
    let mut timezone_value: Option<i32> = None;
    let mut nickname_value: Option<Nickname> = None;

    for param in body.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            let decoded_value = url_decode(value);
            match key {
                "ssid" => {
                    let _ = ssid.push_str(&decoded_value);
                }
                "password" => {
                    let _ = password.push_str(&decoded_value);
                }
                "timezone" => {
                    if let Ok(offset) = decoded_value.parse::<i32>() {
                        timezone_value = Some(offset);
                    } else {
                        warn!("Invalid timezone offset: {}", decoded_value);
                        return None;
                    }
                }
                "nickname" => {
                    if let Some(nickname_state) = &state.nickname {
                        let trimmed = decoded_value.trim();
                        if trimmed.is_empty() {
                            warn!("Nickname missing");
                            return None;
                        }
                        nickname_value = Some(clamp_nickname(trimmed, nickname_state.max_len));
                    }
                }
                _ => {}
            }
        }
    }

    if ssid.is_empty() {
        return None;
    }

    let timezone_offset_minutes = if let Some(tz_state) = &state.timezone {
        timezone_value.or(Some(tz_state.default_offset_minutes))?
    } else {
        0
    };

    if state.timezone.is_some() && timezone_value.is_none() {
        warn!("Timezone field missing in submission");
        return None;
    }

    let nickname = if state.nickname.is_some() {
        nickname_value
    } else {
        None
    };

    Some(WifiCredentialSubmission {
        credentials: WifiCredentials { ssid, password },
        timezone_offset_minutes,
        nickname,
    })
}

fn generate_config_page(state: &FormState) -> heapless::String<4096> {
    let mut page = heapless::String::<4096>::new();
    let ssid = state
        .defaults
        .as_ref()
        .map(|creds| escape_html::<160>(creds.ssid.as_str()))
        .unwrap_or_else(heapless::String::new);
    let password = state
        .defaults
        .as_ref()
        .map(|creds| escape_html::<320>(creds.password.as_str()))
        .unwrap_or_else(heapless::String::new);

    write!(
        page,
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html\r\n\
         Connection: close\r\n\
         \r\n\
         <!DOCTYPE html>\
         <html>\
         <head>\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <title>WiFi Configuration</title>\
             <link rel=\"icon\" href=\"data:,\">\
             <style>\
                 body {{ font-family: Arial, sans-serif; max-width: 500px; margin: 50px auto; padding: 20px; }}\
                 h1 {{ color: #333; }}\
                 form {{ margin-top: 20px; }}\
                 input {{ width: 100%; padding: 10px; margin: 10px 0; box-sizing: border-box; }}\
                 label {{ display: block; margin-top: 10px; }}\
                 .toggle {{ display: flex; align-items: center; gap: 8px; font-size: 0.9rem; color: #444; }}\
                 button {{ width: 100%; padding: 12px; background-color: #4CAF50; color: white; border: none; cursor: pointer; }}\
                 button:hover {{ background-color: #45a049; }}\
             </style>\
             <script>\
                 function togglePasswordVisibility() {{\
                     var input = document.getElementById('password');\
                     input.type = input.type === 'password' ? 'text' : 'password';\
                 }}\
             </script>\
         </head>\
         <body>\
             <h1>WiFi Configuration</h1>\
             <p>Enter your WiFi network credentials:</p>\
             <form method=\"POST\" action=\"/\">\
                 <label for=\"ssid\">WiFi Network Name (SSID):</label>\
                 <input type=\"text\" id=\"ssid\" name=\"ssid\" value=\"{}\" required>\
                 <label for=\"password\">Password:</label>\
                 <input type=\"password\" id=\"password\" name=\"password\" value=\"{}\" required>\
                 <label class=\"toggle\"><input type=\"checkbox\" onclick=\"togglePasswordVisibility()\">Show password</label>\
",
        ssid,
        password
    )
    .ok();

    if let Some(timezone) = &state.timezone {
        append_timezone_select(&mut page, timezone);
    }

    if let Some(nickname) = &state.nickname {
        append_nickname_input(&mut page, nickname);
    }

    let _ = write!(
        page,
        "\
                 <button type=\"submit\">Connect</button>\
             </form>\
         </body>\
         </html>",
    )
    .ok();

    page
}

fn generate_success_page() -> &'static str {
    "HTTP/1.1 200 OK\r\n\
     Content-Type: text/html\r\n\
     Connection: close\r\n\
     \r\n\
     <!DOCTYPE html>\
     <html>\
     <head>\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>Configuration Saved</title>\
         <style>\
             body { font-family: Arial, sans-serif; max-width: 500px; margin: 50px auto; padding: 20px; text-align: center; }\
             h1 { color: #4CAF50; }\
         </style>\
     </head>\
     <body>\
         <h1>Configuration Saved!</h1>\
         <p>WiFi credentials have been received.</p>\
         <p>The device will restart and connect to your network.</p>\
         <p>You can close this page.</p>\
     </body>\
     </html>"
}

fn generate_error_page() -> &'static str {
    "HTTP/1.1 400 Bad Request\r\n\
     Content-Type: text/html\r\n\
     Connection: close\r\n\
     \r\n\
     <!DOCTYPE html>\
     <html>\
     <head>\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>Error</title>\
         <style>\
             body { font-family: Arial, sans-serif; max-width: 500px; margin: 50px auto; padding: 20px; text-align: center; }\
             h1 { color: #f44336; }\
         </style>\
     </head>\
     <body>\
         <h1>Error</h1>\
         <p>Failed to process your request.</p>\
         <p><a href=\"/\">Try again</a></p>\
     </body>\
     </html>"
}

fn url_decode(s: &str) -> heapless::String<64> {
    let mut result = heapless::String::<64>::new();
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '+' {
            let _ = result.push(' ');
        } else if c == '%' {
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                if let (Some(d1), Some(d2)) = (h1.to_digit(16), h2.to_digit(16)) {
                    #[allow(clippy::cast_possible_truncation)]
                    let byte = ((d1 << 4) | d2) as u8;
                    if let Ok(ch) = core::str::from_utf8(&[byte]) {
                        let _ = result.push_str(ch);
                    }
                }
            }
        } else {
            let _ = result.push(c);
        }
    }

    result
}

fn escape_html<const N: usize>(value: &str) -> heapless::String<N> {
    let mut escaped = heapless::String::<N>::new();
    for ch in value.chars() {
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

fn clamp_nickname(value: &str, max_len: usize) -> Nickname {
    let mut nickname = Nickname::new();
    for ch in value.chars() {
        if nickname.len() >= max_len {
            break;
        }
        if nickname.push(ch).is_err() {
            break;
        }
    }
    nickname
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

fn append_timezone_select(page: &mut heapless::String<4096>, timezone: &TimezoneFormState) {
    let _ = write!(
        page,
        "<label for=\"timezone\">Time zone:</label>\
         <select id=\"timezone\" name=\"timezone\" required>"
    );
    for option in TIMEZONE_OPTIONS {
        let selected = if option.minutes == timezone.default_offset_minutes {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            page,
            "<option value=\"{}\"{}>{}</option>",
            option.minutes, selected, option.label
        );
    }
    let _ = page.push_str("</select>");
}

fn append_nickname_input(page: &mut heapless::String<4096>, nickname: &NicknameFormState) {
    let escaped = escape_html::<256>(nickname.value.as_str());
    let _ = write!(
        page,
        "<label for=\"nickname\">User name:</label>\
         <input type=\"text\" id=\"nickname\" name=\"nickname\" value=\"{}\" \
         maxlength=\"{}\" required>",
        escaped, nickname.max_len
    );
}

fn static_page(content: &'static str) -> heapless::String<4096> {
    let mut page = heapless::String::<4096>::new();
    let _ = page.push_str(content);
    page
}

fn empty_favicon_response() -> heapless::String<4096> {
    static_page("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
}

fn redirect_to_root() -> heapless::String<4096> {
    static_page(
        "HTTP/1.1 302 Found\r\nLocation: /\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
}
