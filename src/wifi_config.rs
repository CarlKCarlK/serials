//! WiFi credential collection via captive portal web interface.

#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::{cell::RefCell, fmt::Write};
use defmt::{info, unwrap, warn};
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
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
}

/// Configuration for the WiFi credential form.
#[derive(Clone, Copy, Debug, Default)]
pub struct WifiConfigOptions<'a> {
    pub defaults: Option<&'a WifiCredentials>,
}

impl<'a> WifiConfigOptions<'a> {
    #[must_use]
    pub const fn with_defaults(defaults: Option<&'a WifiCredentials>) -> Self {
        Self { defaults }
    }
}

static CREDENTIAL_CHANNEL: Channel<CriticalSectionRawMutex, WifiCredentialSubmission, 1> =
    Channel::new();

#[derive(Clone)]
struct FormState {
    defaults: Option<WifiCredentials>,
}

static FORM_STATE: Mutex<CriticalSectionRawMutex, RefCell<FormState>> =
    Mutex::new(RefCell::new(FormState { defaults: None }));

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
                if let Some(submission) = parse_credentials_from_post(request_text) {
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

fn parse_credentials_from_post(request: &str) -> Option<WifiCredentialSubmission> {
    let body_start = request.find("\r\n\r\n")? + 4;
    let body = &request[body_start..];

    info!("POST body: {}", body);

    let mut ssid = heapless::String::<32>::new();
    let mut password = heapless::String::<64>::new();

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
                _ => {}
            }
        }
    }

    if !ssid.is_empty() {
        Some(WifiCredentialSubmission {
            credentials: WifiCredentials { ssid, password },
            timezone_offset_minutes: 0,
        })
    } else {
        None
    }
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
                 <button type=\"submit\">Connect</button>\
             </form>\
         </body>\
         </html>",
        ssid,
        password
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

fn static_page(content: &'static str) -> heapless::String<4096> {
    let mut page = heapless::String::<4096>::new();
    let _ = page.push_str(content);
    page
}

fn empty_favicon_response() -> heapless::String<4096> {
    static_page("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
}

fn redirect_to_root() -> heapless::String<4096> {
    static_page("HTTP/1.1 302 Found\r\nLocation: /\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}
