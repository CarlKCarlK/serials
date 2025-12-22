//! WiFi credential collection via captive portal web interface.

#![allow(clippy::future_not_send, reason = "single-threaded")]
#![allow(dead_code, reason = "legacy code kept for reference")]

use defmt::{info, unwrap, warn};
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;

use crate::Result;

// ============================================================================
// Types
// ============================================================================

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

// ============================================================================
// Credential Channel
// ============================================================================

/// Channel for sending credentials from HTTP server to application
static CREDENTIAL_CHANNEL: Channel<CriticalSectionRawMutex, WifiCredentialSubmission, 1> =
    Channel::new();

// ============================================================================
// Public API
// ============================================================================

/// Collect WiFi credentials from user via web interface
///
/// This function spawns the HTTP server task and waits for credentials to be submitted.
/// The HTTP server runs at 192.168.4.1 on port 80.
///
/// # Returns
/// Returns `WifiCredentials` containing the SSID and password entered by the user.
pub async fn collect_wifi_credentials(
    stack: &'static Stack<'static>,
    spawner: embassy_executor::Spawner,
) -> Result<WifiCredentialSubmission> {
    info!("Starting credential collection...");

    // Spawn the HTTP server task
    let token = unwrap!(http_config_server_task(stack));
    spawner.spawn(token);
    info!("HTTP configuration task spawned");

    // Wait for credentials to be submitted
    info!("Waiting for user to submit credentials via web interface...");
    let submission = CREDENTIAL_CHANNEL.receive().await;

    info!("Credentials received!");
    Ok(submission)
}

// ============================================================================
// HTTP Server Task
// ============================================================================

/// HTTP server task for WiFi configuration
///
/// Serves a simple configuration page and accepts WiFi credentials via POST
#[embassy_executor::task]
#[allow(unsafe_code)] // Required for static mut buffers to avoid stack overflow
pub async fn http_config_server_task(stack: &'static Stack<'static>) -> ! {
    info!("HTTP config server starting on port 80");

    // Use static buffers to avoid stack overflow (7KB would be too much for stack)
    static mut RX_BUFFER: [u8; 2048] = [0u8; 2048];
    static mut TX_BUFFER: [u8; 4096] = [0u8; 4096];
    static mut REQUEST_BUFFER: [u8; 1024] = [0u8; 1024];

    // Safety: This is safe because this task is spawned only once and these
    // static muts are only accessed from this single task, never concurrently.
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
                socket.flush().await.ok();
                socket.close();
                continue;
            }
            Ok(n) => n,
            Err(e) => {
                warn!("Read error: {:?}", e);
                socket.flush().await.ok();
                socket.close();
                continue;
            }
        };

        let request_text = core::str::from_utf8(&request[..request_len]).unwrap_or("");
        info!("Got HTTP request ({} bytes)", request_len);

        // Parse request
        let mut lines = request_text.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let _raw_path = parts.next().unwrap_or("/");

        let response = if method == "GET" {
            // Serve the configuration page
            generate_config_page()
        } else if method == "POST" {
            // Process the form submission
            if let Some(submission) = parse_credentials_from_post(request_text) {
                info!("Received WiFi credentials:");
                info!("  SSID: {}", submission.credentials.ssid);
                info!("  Password: [hidden]");
                info!(
                    "  Timezone offset: {} minutes",
                    submission.timezone_offset_minutes
                );

                // Send credentials through channel
                CREDENTIAL_CHANNEL.send(submission).await;

                generate_success_page()
            } else {
                warn!("Failed to parse credentials");
                generate_error_page()
            }
        } else {
            generate_error_page()
        };

        if let Err(e) = socket.write_all(response.as_bytes()).await {
            warn!("Write error: {:?}", e);
        }

        let _ = socket.flush().await;
        socket.close();
        Timer::after_millis(100).await;
    }
}

/// Parse WiFi credentials from POST request body
fn parse_credentials_from_post(request: &str) -> Option<WifiCredentialSubmission> {
    // Find the body (after \r\n\r\n)
    let body_start = request.find("\r\n\r\n")? + 4;
    let body = &request[body_start..];

    info!("POST body: {}", body);

    // Parse form data: ssid=XXX&password=YYY
    let mut ssid = heapless::String::<32>::new();
    let mut password = heapless::String::<64>::new();
    let mut offset_minutes: Option<i32> = None;

    for param in body.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            let decoded_value = url_decode(value);
            match key {
                "ssid" => {
                    ssid.push_str(&decoded_value)
                        .expect("ssid exceeds capacity");
                }
                "password" => {
                    password
                        .push_str(&decoded_value)
                        .expect("password exceeds capacity");
                }
                "offset" => {
                    offset_minutes = decoded_value.parse::<i32>().ok();
                }
                _ => {}
            }
        }
    }

    if !ssid.is_empty() {
        let timezone_offset_minutes = offset_minutes.unwrap_or(0);
        Some(WifiCredentialSubmission {
            credentials: WifiCredentials { ssid, password },
            timezone_offset_minutes,
        })
    } else {
        None
    }
}

/// Simple URL decode (handles %20 -> space, %2B -> +, etc.)
fn url_decode(s: &str) -> heapless::String<64> {
    let mut result = heapless::String::<64>::new();
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '+' {
            result.push(' ').ok();
        } else if c == '%' {
            // Try to parse hex code
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                if let (Some(d1), Some(d2)) = (h1.to_digit(16), h2.to_digit(16)) {
                    #[allow(clippy::cast_possible_truncation)]
                    let byte = ((d1 << 4) | d2) as u8;
                    if let Ok(ch) = core::str::from_utf8(&[byte]) {
                        result.push_str(ch).ok();
                    }
                }
            }
        } else {
            result.push(c).ok();
        }
    }

    result
}

/// Generate HTML configuration page
fn generate_config_page() -> &'static str {
    "HTTP/1.1 200 OK\r\n\
     Content-Type: text/html\r\n\
     Connection: close\r\n\
     \r\n\
     <!DOCTYPE html>\
     <html>\
     <head>\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>WiFi Configuration</title>\
         <style>\
             body { font-family: Arial, sans-serif; max-width: 500px; margin: 50px auto; padding: 20px; }\
             h1 { color: #333; }\
             form { margin-top: 20px; }\
             input { width: 100%; padding: 10px; margin: 10px 0; box-sizing: border-box; }\
             label { display: block; margin-top: 10px; }\
             .toggle { display: flex; align-items: center; gap: 8px; font-size: 0.9rem; color: #444; }\
             button { width: 100%; padding: 12px; background-color: #4CAF50; color: white; border: none; cursor: pointer; }\
             button:hover { background-color: #45a049; }\
         </style>\
         <script>\
             function togglePasswordVisibility() {\
                 var input = document.getElementById('password');\
                 input.type = input.type === 'password' ? 'text' : 'password';\
             }\
         </script>\
     </head>\
     <body>\
         <h1>WiFi Configuration</h1>\
         <p>Enter your WiFi network credentials:</p>\
         <form method=\"POST\" action=\"/\">\
             <label for=\"ssid\">WiFi Network Name (SSID):</label>\
             <input type=\"text\" id=\"ssid\" name=\"ssid\" required>\
             <label for=\"password\">Password:</label>\
             <input type=\"password\" id=\"password\" name=\"password\" required>\
             <label class=\"toggle\"><input type=\"checkbox\" onclick=\"togglePasswordVisibility()\">Show password</label>\
             <label for=\"offset\">Timezone:</label>\
             <select id=\"offset\" name=\"offset\">\
                 <option value=\"-720\">Baker Island (UTC-12:00)</option>\
                 <option value=\"-660\">American Samoa (UTC-11:00)</option>\
                 <option value=\"-600\">Honolulu (UTC-10:00)</option>\
                 <option value=\"-540\">Anchorage, Alaska ST (UTC-09:00)</option>\
                 <option value=\"-480\">Anchorage, Alaska DT (UTC-08:00)</option>\
                 <option value=\"-480\">Los Angeles, San Francisco, Seattle ST (UTC-08:00)</option>\
                 <option value=\"-420\">Los Angeles, San Francisco, Seattle DT (UTC-07:00)</option>\
                 <option value=\"-420\">Denver, Phoenix ST (UTC-07:00)</option>\
                 <option value=\"-360\">Denver DT (UTC-06:00)</option>\
                 <option value=\"-360\">Chicago, Dallas, Mexico City ST (UTC-06:00)</option>\
                 <option value=\"-300\">Chicago, Dallas DT (UTC-05:00)</option>\
                 <option value=\"-300\">New York, Toronto, Bogota ST (UTC-05:00)</option>\
                 <option value=\"-240\">New York, Toronto DT (UTC-04:00)</option>\
                 <option value=\"-240\">Santiago, Halifax ST (UTC-04:00)</option>\
                 <option value=\"-210\">St. John's, Newfoundland ST (UTC-03:30)</option>\
                 <option value=\"-180\">Buenos Aires, Sao Paulo (UTC-03:00)</option>\
                 <option value=\"-120\">South Georgia (UTC-02:00)</option>\
                 <option value=\"-60\">Azores ST (UTC-01:00)</option>\
                 <option value=\"0\" selected>London, Lisbon ST (UTC+00:00)</option>\
                 <option value=\"60\">London, Paris, Berlin DT (UTC+01:00)</option>\
                 <option value=\"60\">Paris, Berlin, Rome ST (UTC+01:00)</option>\
                 <option value=\"120\">Paris, Berlin, Rome DT (UTC+02:00)</option>\
                 <option value=\"120\">Athens, Cairo, Johannesburg ST (UTC+02:00)</option>\
                 <option value=\"180\">Athens DT (UTC+03:00)</option>\
                 <option value=\"180\">Moscow, Istanbul, Nairobi (UTC+03:00)</option>\
                 <option value=\"240\">Dubai, Baku (UTC+04:00)</option>\
                 <option value=\"270\">Tehran ST (UTC+04:30)</option>\
                 <option value=\"300\">Karachi, Tashkent (UTC+05:00)</option>\
                 <option value=\"330\">Mumbai, Delhi (UTC+05:30)</option>\
                 <option value=\"345\">Kathmandu (UTC+05:45)</option>\
                 <option value=\"360\">Dhaka, Almaty (UTC+06:00)</option>\
                 <option value=\"390\">Yangon (UTC+06:30)</option>\
                 <option value=\"420\">Bangkok, Jakarta (UTC+07:00)</option>\
                 <option value=\"480\">Singapore, Hong Kong, Beijing (UTC+08:00)</option>\
                 <option value=\"525\">Eucla, Australia (UTC+08:45)</option>\
                 <option value=\"540\">Tokyo, Seoul (UTC+09:00)</option>\
                 <option value=\"570\">Adelaide ST (UTC+09:30)</option>\
                 <option value=\"600\">Sydney, Melbourne ST (UTC+10:00)</option>\
                 <option value=\"630\">Adelaide DT (UTC+10:30)</option>\
                 <option value=\"660\">Sydney, Melbourne DT (UTC+11:00)</option>\
                 <option value=\"720\">Auckland, Fiji ST (UTC+12:00)</option>\
                 <option value=\"780\">Auckland DT (UTC+13:00)</option>\
                 <option value=\"840\">Kiribati (UTC+14:00)</option>\
             </select>\
             <button type=\"submit\">Connect</button>\
         </form>\
     </body>\
     </html>"
}

/// Generate success page
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

/// Generate error page
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
