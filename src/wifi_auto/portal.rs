use core::{cell::RefCell, fmt::Write};

use defmt::{Debug2Format, info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write as _;
use heapless::{FnvIndexMap, String};
use static_cell::StaticCell;

use crate::Result;
use crate::wifi_config::WifiCredentials;

pub type HtmlBuffer = String<16384>;

/// Trait for custom configuration fields in the WiFi provisioning portal.
///
/// Implement this trait to collect additional configuration beyond WiFi credentials
/// during the captive portal setup. Fields must be `Sync` since they're shared across
/// async tasks.
///
/// See [`TimezoneField`](super::fields::TimezoneField) and
/// [`TextField`](super::fields::TextField) for complete
/// implementation examples.
///
/// # Methods
///
/// - [`render`](Self::render): Generate HTML form elements for the captive portal
/// - [`parse`](Self::parse): Parse and save submitted form data
/// - [`is_satisfied`](Self::is_satisfied): Check if field has valid configuration
pub trait WifiAutoField: Sync {
    /// Render HTML form elements for this field.
    ///
    /// Append form elements (labels, inputs, selects, etc.) to the `page` buffer.
    /// This is called when generating the captive portal page.
    ///
    /// See [`TimezoneField`](super::fields::TimezoneField) and
    /// [`TextField`](super::fields::TextField) for examples.
    fn render(&self, page: &mut HtmlBuffer) -> Result<()>;
    
    /// Parse and save form data submitted by the user.
    ///
    /// Extract values from the `form` data and persist them (typically to flash).
    /// Return an error if validation fails.
    ///
    /// See [`TimezoneField`](super::fields::TimezoneField) and
    /// [`TextField`](super::fields::TextField) for examples.
    fn parse(&self, form: &FormData<'_>) -> Result<()>;
    
    /// Check if this field has valid configuration.
    ///
    /// Returns `true` if the field has been configured (default implementation always
    /// returns `true`). If `false`, the captive portal will be shown even if WiFi
    /// credentials exist.
    ///
    /// See [`TimezoneField`](super::fields::TimezoneField) and
    /// [`TextField`](super::fields::TextField) for examples.
    fn is_satisfied(&self) -> Result<bool> {
        Ok(true)
    }
}

pub struct FormData<'a> {
    params: &'a FormMap,
}

impl<'a> FormData<'a> {
    fn new(params: &'a FormMap) -> Self {
        Self { params }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(stored, _)| stored.as_str() == key)
            .map(|(_, value)| value.as_str())
    }
}

type FormKey = String<32>;
type FormValue = String<256>;
type FormMap = FnvIndexMap<FormKey, FormValue, 32>;

static CREDENTIAL_CHANNEL: Channel<CriticalSectionRawMutex, WifiCredentials, 1> = Channel::new();

#[derive(Clone)]
struct FormState {
    defaults: Option<WifiCredentials>,
}

static FORM_STATE: Mutex<CriticalSectionRawMutex, RefCell<FormState>> =
    Mutex::new(RefCell::new(FormState { defaults: None }));

static FORM_FIELDS: Mutex<CriticalSectionRawMutex, RefCell<&'static [&'static dyn WifiAutoField]>> =
    Mutex::new(RefCell::new(&[]));

pub async fn collect_credentials(
    stack: &'static Stack<'static>,
    spawner: Spawner,
    defaults: Option<&WifiCredentials>,
    fields: &'static [&'static dyn WifiAutoField],
) -> Result<WifiCredentials> {
    info!("WifiAuto portal registering {} custom fields", fields.len());
    FORM_STATE.lock(|state| {
        state.borrow_mut().defaults = defaults.cloned();
    });
    FORM_FIELDS.lock(|slot| {
        *slot.borrow_mut() = fields;
    });

    let token = unwrap!(http_server_task(stack));
    spawner.spawn(token);

    let submission = CREDENTIAL_CHANNEL.receive().await;
    Ok(submission)
}

#[embassy_executor::task]
async fn http_server_task(stack: &'static Stack<'static>) -> ! {
    info!("WifiAuto HTTP portal starting");

    static RX_BUFFER: StaticCell<[u8; 2048]> = StaticCell::new();
    static TX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
    static REQUEST_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();

    let rx_buffer = RX_BUFFER.init([0; 2048]);
    let tx_buffer = TX_BUFFER.init([0; 4096]);
    let request = REQUEST_BUFFER.init([0; 1024]);

    loop {
        let mut socket = TcpSocket::new(*stack, rx_buffer, tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(30)));

        info!("Waiting for HTTP connection...");
        if let Err(err) = socket.accept(80).await {
            warn!("Accept error: {:?}", err);
            Timer::after_millis(500).await;
            continue;
        }

        let request_len = match socket.read(request).await {
            Ok(0) => {
                info!("Client closed connection");
                let _ = socket.flush().await;
                socket.close();
                continue;
            }
            Ok(n) => n,
            Err(err) => {
                warn!("HTTP read error: {:?}", err);
                let _ = socket.flush().await;
                socket.close();
                continue;
            }
        };

        let request_text = core::str::from_utf8(&request[..request_len]).unwrap_or("");
        let mut lines = request_text.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");

        let response = match method {
            "GET" => {
                let state_snapshot = FORM_STATE.lock(|state| state.borrow().clone());
                let fields_snapshot = FORM_FIELDS.lock(|fields| *fields.borrow());
                generate_config_page(&state_snapshot, fields_snapshot)
            }
            "POST" => {
                let fields_snapshot = FORM_FIELDS.lock(|fields| *fields.borrow());
                if let Some(credentials) = parse_post(request_text, fields_snapshot) {
                    CREDENTIAL_CHANNEL.send(credentials).await;
                    static_page(generate_success_page())
                } else {
                    warn!("WifiAuto portal failed to parse POST");
                    static_page(generate_error_page())
                }
            }
            _ => static_page(generate_error_page()),
        };

        if let Err(err) = socket.write_all(response.as_bytes()).await {
            warn!("HTTP write error: {:?}", err);
        }

        let _ = socket.flush().await;
        socket.close();
        Timer::after_millis(100).await;
    }
}

fn parse_post(request: &str, fields: &[&'static dyn WifiAutoField]) -> Option<WifiCredentials> {
    let body_start = request.find("\r\n\r\n")? + 4;
    let body = &request[body_start..];

    let mut params: FormMap = FormMap::new();
    let mut ssid = heapless::String::<32>::new();
    let mut password = heapless::String::<64>::new();

    for param in body.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            let decoded_key = url_decode::<32>(key);
            let decoded_value = url_decode::<256>(value);
            let _ = params.insert(decoded_key.clone(), decoded_value.clone());
            match decoded_key.as_str() {
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

    if ssid.is_empty() {
        return None;
    }

    let form = FormData::new(&params);
    for field in fields {
        if let Err(err) = field.parse(&form) {
            warn!("WifiAuto field parse failed: {}", Debug2Format(&err));
            return None;
        }
    }

    Some(WifiCredentials { ssid, password })
}

fn generate_config_page(state: &FormState, fields: &[&'static dyn WifiAutoField]) -> HtmlBuffer {
    info!("WifiAuto portal rendering {} fields", fields.len());
    let mut page = HtmlBuffer::new();
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
                 .toggle {{ display: flex; align-items: center; gap: 8px; font-size: 0.9rem; color: #444; margin-top: 5px; }}\
                 .toggle input {{ width: auto; margin: 0; }}\
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
        ssid, password
    )
    .ok();

    for field in fields {
        if let Err(err) = field.render(&mut page) {
            warn!("WifiAuto field render failed: {}", Debug2Format(&err));
        }
    }

    let _ = page.push_str("<button type=\"submit\">Connect</button></form></body></html>");

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

fn static_page(content: &'static str) -> HtmlBuffer {
    let mut page = HtmlBuffer::new();
    let _ = page.push_str(content);
    page
}

fn url_decode<const N: usize>(s: &str) -> heapless::String<N> {
    let mut result = heapless::String::<N>::new();
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
