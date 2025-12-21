//! Pre-built field implementations for [`WifiSetupField`].
//!
//! This module provides ready-to-use field types that can be passed to
//! [`WifiSetup::new()`](super::WifiSetup::new) for collecting additional
//! configuration beyond WiFi credentials.
//!
//! See [`TimezoneField`] and [`TextField`] for complete examples of implementing custom fields.

#![allow(
    unsafe_code,
    reason = "unsafe impl Sync is sound: single-threaded Embassy executor, no concurrent access"
)]

use core::{cell::RefCell, fmt::Write as FmtWrite};
use defmt::info;
use heapless::String;
use static_cell::StaticCell;

use super::portal::{FormData, HtmlBuffer, WifiSetupField};
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
/// # #![no_std]
/// # #![no_main]
/// use device_kit::button::PressedTo;
/// use device_kit::flash_array::{FlashArray, FlashArrayStatic, FlashBlock};
/// use device_kit::wifi_setup::{WifiSetup, WifiSetupStatic};
/// use device_kit::wifi_setup::fields::{TimezoneField, TimezoneFieldStatic};
/// # #[panic_handler]
/// # fn panic(_info: &core::panic::PanicInfo) -> ! { loop {} }
/// async fn example(
///     spawner: embassy_executor::Spawner,
///     p: embassy_rp::Peripherals,
/// ) -> Result<(), device_kit::Error> {
///     // Set up flash storage
///     static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
///     let [wifi_flash, timezone_flash] =
///         FlashArray::new(&FLASH_STATIC, p.FLASH)?;
///
///     // Create timezone field
///     static TIMEZONE_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
///     let timezone_field = TimezoneField::new(&TIMEZONE_STATIC, timezone_flash);
///
///     // Pass to WifiSetup
///     static wifi_setup_STATIC: WifiSetupStatic = WifiSetup::new_static();
///     let wifi_setup = WifiSetup::new(
///         &wifi_setup_STATIC,
///         p.PIN_23,
///         p.PIN_25,
///         p.PIO0,
///         p.PIN_24,
///         p.PIN_29,
///         p.DMA_CH0,
///         wifi_flash,
///         p.PIN_13,
///         PressedTo::Ground,
///         "ClockStation",
///         [timezone_field],  // Custom fields array
///         spawner,
///     )?;
///
///     // Later, retrieve the timezone offset
///     let offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
///     Ok(())
/// }
/// ```
pub struct TimezoneField {
    flash: RefCell<FlashBlock>,
}

// SAFETY: TimezoneField is used in a single-threaded Embassy executor on RP2040/RP2350.
// There are no interrupts that access this data, and all async operations are cooperative
// (non-preemptive). The Sync bound is required only because WifiSetupField trait objects
// are stored in static storage, not because of actual concurrent access.
unsafe impl Sync for TimezoneField {}

/// Static for [`TimezoneField`]. See [`TimezoneField`] for usage example.
pub struct TimezoneFieldStatic {
    cell: StaticCell<TimezoneField>,
}

impl TimezoneFieldStatic {
    const fn new() -> Self {
        Self {
            cell: StaticCell::new(),
        }
    }
}

impl TimezoneField {
    /// Create static resources for [`TimezoneField`].
    ///
    /// See [`TimezoneField`] for a complete example.
    pub const fn new_static() -> TimezoneFieldStatic {
        TimezoneFieldStatic::new()
    }

    /// Initialize a new timezone field.
    ///
    /// See [`TimezoneField`] for a complete example.
    pub fn new(
        timezone_field_static: &'static TimezoneFieldStatic,
        flash: FlashBlock,
    ) -> &'static Self {
        timezone_field_static.cell.init(Self::from_flash(flash))
    }

    fn from_flash(flash: FlashBlock) -> Self {
        Self {
            flash: RefCell::new(flash),
        }
    }

    /// Load the stored timezone offset in minutes from UTC.
    ///
    /// Returns `None` if no timezone has been configured yet.
    ///
    /// See [`TimezoneField`] for a complete example.
    pub fn offset_minutes(&self) -> Result<Option<i32>> {
        self.flash.borrow_mut().load::<i32>()
    }

    /// Save a new timezone offset in minutes from UTC to flash.
    ///
    /// This method allows programmatic updates to the timezone, such as when
    /// the user adjusts the timezone via button presses or other UI interactions.
    ///
    /// Only writes to flash if the value has changed, avoiding unnecessary flash wear.
    ///
    /// Alternatively, you can access the underlying flash block directly for
    /// more control over flash operations.
    pub fn set_offset_minutes(&self, offset: i32) -> Result<()> {
        let current = self.offset_minutes()?;
        if current != Some(offset) {
            self.flash.borrow_mut().save(&offset)?;
        }
        Ok(())
    }

    /// Clear the stored timezone offset, returning the field to an unconfigured state.
    pub fn clear(&self) -> Result<()> {
        self.flash.borrow_mut().clear()
    }
}

impl WifiSetupField for TimezoneField {
    fn render(&self, page: &mut HtmlBuffer) -> Result<()> {
        info!("WifiSetup field: rendering timezone select");
        let current = self.offset_minutes()?.unwrap_or(0);
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
        self.set_offset_minutes(offset)
    }

    fn is_satisfied(&self) -> Result<bool> {
        Ok(self.offset_minutes()?.is_some())
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

/// A generic text input field for collecting user input during WiFi provisioning.
///
/// Presents a customizable text input box in the captive portal that validates and stores
/// user-provided text to flash. Can be used for device names, locations, or any other
/// text-based configuration.
///
/// Multiple `TextField` instances can be created with different labels and field names
/// to collect various pieces of information during the provisioning process.
///
/// # Example
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// use device_kit::button::PressedTo;
/// use device_kit::flash_array::{FlashArray, FlashArrayStatic, FlashBlock};
/// use device_kit::wifi_setup::{WifiSetup, WifiSetupStatic};
/// use device_kit::wifi_setup::fields::{TextField, TextFieldStatic};
/// # #[panic_handler]
/// # fn panic(_info: &core::panic::PanicInfo) -> ! { loop {} }
/// async fn example(
///     spawner: embassy_executor::Spawner,
///     p: embassy_rp::Peripherals,
/// ) -> Result<(), device_kit::Error> {
///     // Set up flash storage
///     static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
///     let [wifi_flash, device_name_flash] =
///         FlashArray::new(&FLASH_STATIC, p.FLASH)?;
///
///     // Create device name field (max 32 chars)
///     static DEVICE_NAME_STATIC: TextFieldStatic<32> = TextField::new_static();
///     let device_name_field = TextField::new(
///         &DEVICE_NAME_STATIC,
///         device_name_flash,
///         "device_name",    // HTML field name
///         "Device Name",    // Label text
///         "Pico",           // Default value
///     );
///
///     // Pass to WifiSetup
///     static wifi_setup_STATIC: WifiSetupStatic = WifiSetup::new_static();
///     let wifi_setup = WifiSetup::new(
///         &wifi_setup_STATIC,
///         p.PIN_23,
///         p.PIN_25,
///         p.PIO0,
///         p.PIN_24,
///         p.PIN_29,
///         p.DMA_CH0,
///         wifi_flash,
///         p.PIN_13,
///         PressedTo::Ground,
///         "Pico",
///         [device_name_field],  // Custom fields array
///         spawner,
///     )?;
///
///     // Later, retrieve the device name
///     let device_name = device_name_field.text()?.unwrap_or_default();
///     Ok(())
/// }
/// ```
pub struct TextField<const N: usize> {
    flash: RefCell<FlashBlock>,
    field_name: &'static str,
    label: &'static str,
    default_value: &'static str,
}

// SAFETY: TextField is used in a single-threaded Embassy executor on RP2040/RP2350.
// There are no interrupts that access this data, and all async operations are cooperative
// (non-preemptive). The Sync bound is required only because WifiSetupField trait objects
// are stored in static storage, not because of actual concurrent access.
unsafe impl<const N: usize> Sync for TextField<N> {}

/// Static for [`TextField`]. See [`TextField`] for usage example.
pub struct TextFieldStatic<const N: usize> {
    cell: StaticCell<TextField<N>>,
}

impl<const N: usize> TextFieldStatic<N> {
    const fn new() -> Self {
        Self {
            cell: StaticCell::new(),
        }
    }
}

impl<const N: usize> TextField<N> {
    /// Create static resources for [`TextField`].
    ///
    /// See [`TextField`] for a complete example.
    pub const fn new_static() -> TextFieldStatic<N> {
        TextFieldStatic::new()
    }

    /// Initialize a new text input field.
    ///
    /// # Parameters
    /// - `text_field_static`: Static resources for initialization
    /// - `flash`: Flash block for persistent storage
    /// - `field_name`: HTML form field name (e.g., "device_name", "location")
    /// - `label`: HTML label text (e.g., "Device Name:", "Location:")
    /// - `default_value`: Initial value if nothing saved
    ///
    /// The maximum length is determined by the generic parameter `N`.
    ///
    /// See [`TextField`] for a complete example.
    pub fn new(
        text_field_static: &'static TextFieldStatic<N>,
        flash: FlashBlock,
        field_name: &'static str,
        label: &'static str,
        default_value: &'static str,
    ) -> &'static Self {
        text_field_static
            .cell
            .init(Self::from_flash(flash, field_name, label, default_value))
    }

    fn from_flash(
        flash: FlashBlock,
        field_name: &'static str,
        label: &'static str,
        default_value: &'static str,
    ) -> Self {
        Self {
            flash: RefCell::new(flash),
            field_name,
            label,
            default_value,
        }
    }

    /// Load the stored text from flash.
    ///
    /// Returns `None` if no text has been configured yet.
    ///
    /// See [`TextField`] for a complete example.
    pub fn text(&self) -> Result<Option<String<N>>> {
        self.flash.borrow_mut().load::<String<N>>()
    }

    /// Save new text to flash.
    ///
    /// This method allows programmatic updates to the field value, such as when
    /// the user modifies configuration via button presses or other UI interactions.
    ///
    /// The text must not exceed the maximum length `N` specified in the type parameter.
    ///
    /// Alternatively, you can access the underlying flash block directly for
    /// more control over flash operations.
    pub fn set_text(&self, text: &String<N>) -> Result<()> {
        self.flash.borrow_mut().save(text)
    }
}

impl<const N: usize> WifiSetupField for TextField<N> {
    fn render(&self, page: &mut HtmlBuffer) -> Result<()> {
        info!("WifiSetup field: rendering text input");
        let current = self
            .text()?
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                let mut text = String::<N>::new();
                text.push_str(self.default_value)
                    .expect("default value exceeds capacity");
                text
            });
        let escaped = simple_escape(current.as_str());
        FmtWrite::write_fmt(
            page,
            format_args!(
                "<label for=\"{}\">{}:</label>\
                 <input type=\"text\" id=\"{}\" name=\"{}\" value=\"{}\" \
                 maxlength=\"{}\" required>",
                self.field_name, self.label, self.field_name, self.field_name, escaped, N
            ),
        )
        .map_err(|_| Error::FormatError)?;
        Ok(())
    }

    fn parse(&self, form: &FormData<'_>) -> Result<()> {
        let Some(value) = form.get(self.field_name) else {
            info!("WifiSetup field: text input missing from submission");
            return Ok(());
        };
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > N {
            return Err(Error::FormatError);
        }
        let mut text = String::<N>::new();
        text.push_str(trimmed).map_err(|_| Error::FormatError)?;
        self.set_text(&text)
    }

    fn is_satisfied(&self) -> Result<bool> {
        Ok(self.text()?.map_or(false, |text| !text.is_empty()))
    }
}

fn simple_escape(input: &str) -> String<128> {
    let mut escaped = String::<128>::new();
    for ch in input.chars() {
        match ch {
            '&' => {
                escaped
                    .push_str("&amp;")
                    .expect("escaped text exceeds capacity");
            }
            '<' => {
                escaped
                    .push_str("&lt;")
                    .expect("escaped text exceeds capacity");
            }
            '>' => {
                escaped
                    .push_str("&gt;")
                    .expect("escaped text exceeds capacity");
            }
            '"' => {
                escaped
                    .push_str("&quot;")
                    .expect("escaped text exceeds capacity");
            }
            '\'' => {
                escaped
                    .push_str("&#39;")
                    .expect("escaped text exceeds capacity");
            }
            _ => {
                escaped.push(ch).expect("escaped text exceeds capacity");
            }
        }
    }
    escaped
}
