//! Pre-built field implementations for [`WifiAutoField`](super::WifiAutoField).
//!
//! This module provides ready-to-use field types that can be passed to
//! [`WifiAuto::new()`](super::WifiAuto::new) for collecting additional
//! configuration beyond WiFi credentials.
//!
//! See [`TimezoneField`] for a complete example of implementing custom fields.

use core::{cell::RefCell, fmt::Write as FmtWrite};
use defmt::info;
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use heapless::String;
use static_cell::StaticCell;

use super::portal::{FormData, HtmlBuffer, WifiAutoField};
use crate::flash_array::FlashBlock;
use crate::{Error, Result};

/// A timezone selection field for WiFi provisioning.
///
/// Allows users to select their timezone from a dropdown during the captive portal
/// setup. The selected offset (in minutes from UTC) is persisted to flash and can
/// be retrieved later.
///
/// # Example
///
/// ```no_run
/// # use serials::flash_array::{FlashArray, FlashArrayNotifier, FlashBlock};
/// # use serials::wifi_auto::{WifiAuto, WifiAutoNotifier};
/// # use serials::wifi_auto::fields::{TimezoneField, TimezoneFieldNotifier};
/// # use embassy_executor::Spawner;
/// # use embassy_rp::peripherals;
/// # async fn example(
/// #     spawner: Spawner,
/// #     peripherals: peripherals::Peripherals,
/// # ) -> Result<(), serials::Error> {
/// // Set up flash storage
/// static FLASH_NOTIFIER: FlashArrayNotifier = FlashArray::<2>::notifier();
/// let [wifi_flash, timezone_flash] =
///     FlashArray::new(&FLASH_NOTIFIER, peripherals.FLASH)?;
///
/// // Create timezone field
/// static TIMEZONE_NOTIFIER: TimezoneFieldNotifier = TimezoneField::notifier();
/// let timezone_field = TimezoneField::new(&TIMEZONE_NOTIFIER, timezone_flash);
///
/// // Pass to WifiAuto
/// static WIFI_AUTO_NOTIFIER: WifiAutoNotifier = WifiAuto::notifier();
/// let wifi_auto = WifiAuto::new(
///     &WIFI_AUTO_NOTIFIER,
///     peripherals.PIN_23,
///     peripherals.PIN_25,
///     peripherals.PIO0,
///     peripherals.PIN_24,
///     peripherals.PIN_29,
///     peripherals.DMA_CH0,
///     wifi_flash,
///     peripherals.PIN_13,
///     "MyDevice",
///     [timezone_field],  // Custom fields array
///     spawner,
/// )?;
///
/// // Later, retrieve the timezone offset
/// let offset_minutes = timezone_field.load_offset()?.unwrap_or(0);
/// # Ok(())
/// # }
/// ```
pub struct TimezoneField {
    flash: Mutex<CriticalSectionRawMutex, RefCell<FlashBlock>>,
}

/// Notifier for [`TimezoneField`]. See [`TimezoneField`] for usage example.
pub struct TimezoneFieldNotifier {
    cell: StaticCell<TimezoneField>,
}

impl TimezoneFieldNotifier {
    const fn new() -> Self {
        Self {
            cell: StaticCell::new(),
        }
    }
}

impl TimezoneField {
    /// Create a new notifier for [`TimezoneField`].
    ///
    /// See [`TimezoneField`] for a complete example.
    pub const fn notifier() -> TimezoneFieldNotifier {
        TimezoneFieldNotifier::new()
    }

    /// Initialize a new timezone field.
    ///
    /// See [`TimezoneField`] for a complete example.
    pub fn new(notifier: &'static TimezoneFieldNotifier, flash: FlashBlock) -> &'static Self {
        notifier.cell.init(Self::from_flash(flash))
    }

    fn from_flash(flash: FlashBlock) -> Self {
        Self {
            flash: Mutex::new(RefCell::new(flash)),
        }
    }

    /// Load the stored timezone offset in minutes from UTC.
    ///
    /// Returns `None` if no timezone has been configured yet.
    ///
    /// See [`TimezoneField`] for a complete example.
    pub fn load_offset(&self) -> Result<Option<i32>> {
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
            FmtWrite::write_fmt(
                page,
                format_args!(
                    "<option value=\"{}\"{}>{}</option>",
                    option.minutes, selected, option.label
                ),
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

/// Maximum length for user names.
const MAX_USER_NAME_LEN: usize = 32;

/// Type alias for user name strings.
type UserName = String<MAX_USER_NAME_LEN>;

/// A text input field for collecting a user name during WiFi provisioning.
///
/// Presents a text input box in the captive portal that validates and stores
/// a user-provided name to flash. Useful for device identification or personalization.
///
/// See [`TimezoneField`] for a complete usage example (the pattern is identical).
pub struct UserNameField {
    flash: Mutex<CriticalSectionRawMutex, RefCell<FlashBlock>>,
    max_len: usize,
    placeholder: &'static str,
}

/// Notifier for [`UserNameField`]. See [`TimezoneField`] for usage example.
pub struct UserNameFieldNotifier {
    cell: StaticCell<UserNameField>,
}

impl UserNameFieldNotifier {
    const fn new() -> Self {
        Self {
            cell: StaticCell::new(),
        }
    }
}

impl UserNameField {
    /// Create a new notifier for [`UserNameField`].
    ///
    /// See [`TimezoneField`] for a complete example.
    pub const fn notifier() -> UserNameFieldNotifier {
        UserNameFieldNotifier::new()
    }

    /// Initialize a new user name field.
    ///
    /// # Parameters
    /// - `notifier`: Static notifier for initialization
    /// - `flash`: Flash block for persistent storage
    /// - `placeholder`: Default text shown in the input field
    /// - `max_len`: Maximum allowed length (must be ≤ [`MAX_USER_NAME_LEN`])
    ///
    /// See [`TimezoneField`] for a complete example.
    pub fn new(
        notifier: &'static UserNameFieldNotifier,
        flash: FlashBlock,
        placeholder: &'static str,
        max_len: usize,
    ) -> &'static Self {
        notifier
            .cell
            .init(Self::from_flash(flash, placeholder, max_len))
    }

    fn from_flash(flash: FlashBlock, placeholder: &'static str, max_len: usize) -> Self {
        Self {
            flash: Mutex::new(RefCell::new(flash)),
            max_len,
            placeholder,
        }
    }

    /// Load the stored user name from flash.
    ///
    /// Returns `None` if no name has been configured yet.
    ///
    /// See [`TimezoneField`] for a complete example.
    pub fn load_name(&self) -> Result<Option<UserName>> {
        self.flash.lock(|cell| cell.borrow_mut().load::<UserName>())
    }

    fn save_name(&self, name: &UserName) -> Result<()> {
        self.flash.lock(|cell| cell.borrow_mut().save(name))
    }

    /// Returns the default/placeholder name.
    ///
    /// See [`TimezoneField`] for a complete example.
    pub fn default_name(&self) -> UserName {
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
        FmtWrite::write_fmt(
            page,
            format_args!(
                "<label for=\"nickname\">User name:</label>\
                 <input type=\"text\" id=\"nickname\" name=\"nickname\" value=\"{}\" \
                 maxlength=\"{}\" required>",
                escaped, self.max_len
            ),
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
