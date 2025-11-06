//! WiFi Configuration Module - Handles AP mode setup and credential collection
//!
//! This module provides functionality to:
//! 1. Start WiFi in AP (Access Point) mode with DHCP server
//! 2. Serve HTTP configuration page for WiFi credentials
//! 3. Accept SSID and password from user via web form
//!
//! The AP runs at 192.168.4.1 with DHCP serving addresses 192.168.4.2-254
//!
//! TODO: List local WiFi networks for user selection
//! TODO: Save credentials between reboots (but not forever)

#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::{info, unwrap, warn};
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;

use crate::Result;

// ============================================================================
// Types
// ============================================================================

/// WiFi credentials collected from user
#[derive(Debug, Clone)]
pub struct WifiCredentials {
    pub ssid: heapless::String<32>,
    pub password: heapless::String<64>,
}

// ============================================================================
// Credential Channel
// ============================================================================

/// Channel for sending credentials from HTTP server to application
static CREDENTIAL_CHANNEL: Channel<CriticalSectionRawMutex, WifiCredentials, 1> = Channel::new();

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
) -> Result<WifiCredentials> {
    info!("Starting credential collection...");
    
    // Spawn the HTTP server task
    let token = unwrap!(http_config_server_task(stack));
    spawner.spawn(token);
    info!("HTTP configuration task spawned");
    
    // Wait for credentials to be submitted
    info!("Waiting for user to submit credentials via web interface...");
    let credentials = CREDENTIAL_CHANNEL.receive().await;
    
    info!("Credentials received!");
    Ok(credentials)
}

// ============================================================================
// HTTP Server Task
// ============================================================================

/// HTTP server task for WiFi configuration
/// 
/// Serves a simple configuration page and accepts WiFi credentials via POST
#[embassy_executor::task]
pub async fn http_config_server_task(stack: &'static Stack<'static>) -> ! {
    info!("HTTP config server starting on port 80");
    
    let mut rx_buffer = [0u8; 2048];
    let mut tx_buffer = [0u8; 4096];
    let mut request = [0u8; 1024];

    loop {
        let mut socket = TcpSocket::new(*stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(30)));

        info!("Waiting for HTTP connection...");
        if let Err(e) = socket.accept(80).await {
            warn!("Accept error: {:?}", e);
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
            if let Some(credentials) = parse_credentials_from_post(request_text) {
                info!("Received WiFi credentials:");
                info!("  SSID: {}", credentials.ssid);
                info!("  Password: [hidden]");
                
                // Send credentials through channel
                CREDENTIAL_CHANNEL.send(credentials).await;
                
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
fn parse_credentials_from_post(request: &str) -> Option<WifiCredentials> {
    // Find the body (after \r\n\r\n)
    let body_start = request.find("\r\n\r\n")? + 4;
    let body = &request[body_start..];

    info!("POST body: {}", body);

    // Parse form data: ssid=XXX&password=YYY
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
        Some(WifiCredentials { ssid, password })
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
             button { width: 100%; padding: 12px; background-color: #4CAF50; color: white; border: none; cursor: pointer; }\
             button:hover { background-color: #45a049; }\
         </style>\
     </head>\
     <body>\
         <h1>WiFi Configuration</h1>\
         <p>Enter your WiFi network credentials:</p>\
         <form method=\"POST\" action=\"/\">\
             <label for=\"ssid\">WiFi Network Name (SSID):</label>\
             <input type=\"text\" id=\"ssid\" name=\"ssid\" required>\
             <label for=\"password\">Password:</label>\
             <input type=\"password\" id=\"password\" name=\"password\" required>\
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
