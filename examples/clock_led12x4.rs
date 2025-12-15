//! Wi-Fi enabled 4-character LED matrix clock (12x4 pixels) with captive-portal setup.
//!
//! This example mirrors the WiFi/clock state machine from `clock_servos.rs` but drives a
//! 12x4 LED panel on GPIO3 instead of servos. The reset button is on GPIO13.
// cmk does the wifi device abstraction know about both kinds of buttons

#![no_std]
#![no_main]
#![cfg(feature = "wifi")]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::cell::RefCell;
use core::pin::pin;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use panic_probe as _;
use serials::button::{Button, ButtonConnection, PressDuration};
use serials::clock::{Clock, ClockStatic, ONE_MINUTE, ONE_SECOND, h12_m_s};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::led_strip_simple::{LedStripSimple, LedStripSimpleStatic, colors};
use serials::led12x4::Led12x4;
use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use serials::wifi_setup::fields::{TimezoneField, TimezoneFieldStatic};
use serials::wifi_setup::{WifiSetup, WifiSetupStatic};
use serials::{Error, Result};
use smart_leds::RGB8;

// cmk use the colors enum
// cmk use an array of colors
// cmk should edit to blicking or colors

const FAST_MODE_SPEED: f32 = 720.0;
const PORTAL_COLOR: RGB8 = colors::NAVY;
const CONNECTING_COLOR: RGB8 = colors::SADDLE_BROWN;
const DIGIT_COLORS: [RGB8; 4] = [colors::NAVY, colors::GREEN, colors::TEAL, colors::MAROON];
const EDIT_COLORS: [RGB8; 4] = [
    colors::FIREBRICK,
    colors::DARK_ORANGE,
    colors::TEAL,
    colors::MAROON,
];

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("Starting Wi-Fi 12x4 LED clock (WifiSetup)");
    let peripherals = embassy_rp::init(Default::default());

    // Use two blocks of flash storage: Wi-Fi credentials + timezone
    static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
    let [wifi_credentials_flash_block, timezone_flash_block] =
        FlashArray::new(&FLASH_STATIC, peripherals.FLASH)?;

    // Define HTML to ask for timezone on the captive portal.
    static TIMEZONE_FIELD_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
    let timezone_field = TimezoneField::new(&TIMEZONE_FIELD_STATIC, timezone_flash_block);

    // Set up Wifi via a captive portal. The button pin is used to reset stored credentials.
    static WIFI_SETUP_STATIC: WifiSetupStatic = WifiSetup::new_static();
    let wifi_setup = WifiSetup::new(
        &WIFI_SETUP_STATIC,
        peripherals.PIN_23,  // CYW43 power
        peripherals.PIN_25,  // CYW43 chip select
        peripherals.PIO0,    // CYW43 PIO interface
        peripherals.PIN_24,  // CYW43 clock
        peripherals.PIN_29,  // CYW43 data pin
        peripherals.DMA_CH0, // CYW43 DMA channel
        wifi_credentials_flash_block,
        peripherals.PIN_13,  // Reset button pin
        "www.picoclock.net", // Captive-portal SSID
        [timezone_field],    // Custom fields to ask for
        spawner,
    )?;
    // cmk pico1 or pico2 button?

    // Set up the 12x4 LED display on GPIO3 using LedStripSimple on PIO1.
    static LED_STRIP_STATIC: LedStripSimpleStatic<48> = LedStripSimpleStatic::new_static();
    let led_strip = LedStripSimple::new_pio1(
        &LED_STRIP_STATIC,
        peripherals.PIO1,
        peripherals.PIN_3,
        500, // 500mA budget allows ~22% brightness for 48 LEDs
    );
    let led_display = RefCell::new(Led12x4ClockDisplay::new(Led12x4::new(led_strip)));

    // Connect Wi-Fi, using the LED panel for status.
    let led_display_ref = &led_display;
    let (stack, mut button) = wifi_setup
        .connect(spawner, move |event| {
            let led_display_ref = led_display_ref;
            async move {
                use serials::wifi_setup::WifiSetupEvent;
                match event {
                    // cmk these message are not as expected
                    WifiSetupEvent::CaptivePortalReady => {
                        led_display_ref
                            .borrow_mut()
                            .show_portal_ready()
                            .await
                            .expect("LED display failed during portal-ready");
                    }
                    WifiSetupEvent::Connecting {
                        try_index,
                        try_count,
                    } => {
                        led_display_ref
                            .borrow_mut()
                            .show_connecting(try_index, try_count)
                            .await
                            .expect("LED display failed during connecting");
                    }
                    WifiSetupEvent::Connected => {
                        // No-op; main loop will immediately render real time.
                    }
                }
            }
        })
        .await?;

    // Reclaim ownership of the display for the main clock loop.
    let mut led_display = led_display.into_inner();

    info!("WiFi connected");

    // Every hour, check the time and fire an event.
    static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    let time_sync = TimeSync::new(&TIME_SYNC_STATIC, stack, spawner);

    // Read the timezone offset, an extra field that WiFi portal saved to flash.
    let offset_minutes = timezone_field
        .offset_minutes()?
        .ok_or(Error::StorageCorrupted)?;

    // Create a headless Clock device that knows its timezone offset.
    static CLOCK_STATIC: ClockStatic = Clock::new_static();
    let clock = Clock::new(&CLOCK_STATIC, offset_minutes, Some(ONE_MINUTE), spawner);

    // Start in HH:MM mode
    let mut state = State::HoursMinutes { speed: 1.0 };
    loop {
        state = match state {
            State::HoursMinutes { speed } => {
                state
                    .execute_hours_minutes(speed, &clock, &mut button, &time_sync, &mut led_display)
                    .await?
            }
            State::MinutesSeconds => {
                state
                    .execute_minutes_seconds(&clock, &mut button, &time_sync, &mut led_display)
                    .await?
            }
            State::EditOffset => {
                state
                    .execute_edit_offset(&clock, &mut button, &timezone_field, &mut led_display)
                    .await?
            }
        };
    }
}

// State machine for 24x4 LED clock display modes and transitions.

/// Display states for the 24x4 LED clock.
#[derive(Debug, defmt::Format, Clone, Copy, PartialEq)]
pub enum State {
    HoursMinutes { speed: f32 },
    MinutesSeconds,
    EditOffset,
}

impl State {
    async fn execute_hours_minutes(
        self,
        speed: f32,
        clock: &Clock,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        led_display: &mut Led12x4ClockDisplay,
    ) -> Result<Self> {
        clock.set_speed(speed).await;
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        led_display.show_hours_minutes(hours, minutes).await?;
        clock.set_tick_interval(Some(ONE_MINUTE)).await;
        let mut button_press = pin!(button.wait_for_press_duration());
        loop {
            match select(
                &mut button_press,
                select(clock.wait_for_tick(), time_sync.wait_for_sync()),
            )
            .await
            {
                // Button pushes
                Either::First(press_duration) => {
                    info!(
                        "HoursMinutes: Button press detected: {:?}, speed_bits={}",
                        press_duration,
                        speed.to_bits()
                    );
                    match (press_duration, speed.to_bits()) {
                        (PressDuration::Short, bits) if bits == 1.0f32.to_bits() => {
                            info!("HoursMinutes -> MinutesSeconds");
                            return Ok(Self::MinutesSeconds);
                        }
                        (PressDuration::Short, _) => {
                            info!("HoursMinutes: Resetting speed to 1.0");
                            return Ok(Self::HoursMinutes { speed: 1.0 });
                        }
                        (PressDuration::Long, _) => {
                            info!("HoursMinutes -> EditOffset");
                            return Ok(Self::EditOffset);
                        }
                    }
                }
                // Clock tick
                Either::Second(Either::First(time_event)) => {
                    let (hours, minutes, _) = h12_m_s(&time_event);
                    led_display.show_hours_minutes(hours, minutes).await?;
                }
                // Time sync events
                Either::Second(Either::Second(TimeSyncEvent::Success { unix_seconds })) => {
                    info!(
                        "Time sync success: setting clock to {}",
                        unix_seconds.as_i64()
                    );
                    clock.set_utc_time(unix_seconds).await;
                }
                Either::Second(Either::Second(TimeSyncEvent::Failed(msg))) => {
                    info!("Time sync failed: {}", msg);
                }
            }
        }
    }

    async fn execute_minutes_seconds(
        self,
        clock: &Clock,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        led_display: &mut Led12x4ClockDisplay,
    ) -> Result<Self> {
        clock.set_speed(1.0).await;
        let (_, minutes, seconds) = h12_m_s(&clock.now_local());
        led_display.show_minutes_seconds(minutes, seconds).await?;
        clock.set_tick_interval(Some(ONE_SECOND)).await;
        loop {
            match select(
                select(button.wait_for_press_duration(), clock.wait_for_tick()),
                time_sync.wait_for_sync(),
            )
            .await
            {
                // Button pushes
                Either::First(Either::First(press_duration)) => {
                    info!(
                        "MinutesSeconds: Button press detected: {:?}",
                        press_duration
                    );
                    match press_duration {
                        PressDuration::Short => {
                            info!("MinutesSeconds -> HoursMinutes (fast)");
                            return Ok(Self::HoursMinutes {
                                speed: FAST_MODE_SPEED,
                            });
                        }
                        PressDuration::Long => {
                            info!("MinutesSeconds -> EditOffset");
                            return Ok(Self::EditOffset);
                        }
                    }
                }
                // Clock tick
                Either::First(Either::Second(time_event)) => {
                    let (_, minutes, seconds) = h12_m_s(&time_event);
                    led_display.show_minutes_seconds(minutes, seconds).await?;
                }
                // Time sync events
                Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                    info!(
                        "Time sync success: setting clock to {}",
                        unix_seconds.as_i64()
                    );
                    clock.set_utc_time(unix_seconds).await;
                }
                Either::Second(TimeSyncEvent::Failed(msg)) => {
                    info!("Time sync failed: {}", msg);
                }
            }
        }
    }

    async fn execute_edit_offset(
        self,
        clock: &Clock,
        button: &mut Button<'_>,
        timezone_field: &TimezoneField,
        led_display: &mut Led12x4ClockDisplay,
    ) -> Result<Self> {
        info!("Entering edit offset mode");
        clock.set_speed(1.0).await;

        // Blink current hours and minutes with edit color accent.
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        led_display
            .show_hours_minutes_indicator(hours, minutes)
            .await?;

        // Get the current offset minutes from clock (source of truth)
        let mut offset_minutes = clock.offset_minutes();
        info!("Current offset: {} minutes", offset_minutes);

        clock.set_tick_interval(None).await; // Disable ticks in edit mode
        loop {
            info!("Waiting for button press in edit mode");
            match button.wait_for_press_duration().await {
                PressDuration::Short => {
                    info!("Short press detected - incrementing offset");
                    // Increment the offset by 1 hour
                    offset_minutes += 60;
                    const ONE_DAY_MINUTES: i32 = serials::clock::ONE_DAY.as_secs() as i32 / 60;
                    if offset_minutes >= ONE_DAY_MINUTES {
                        offset_minutes -= ONE_DAY_MINUTES;
                    }
                    clock.set_offset_minutes(offset_minutes).await;
                    info!("New offset: {} minutes", offset_minutes);

                    // Update display (atomic already updated, can use now_local)
                    let (hours, minutes, _) = h12_m_s(&clock.now_local());
                    info!(
                        "Updated time after offset change: {:02}:{:02}",
                        hours, minutes
                    );
                    led_display
                        .show_hours_minutes_indicator(hours, minutes)
                        .await?;
                }
                PressDuration::Long => {
                    info!("Long press detected - saving and exiting edit mode");
                    // Save to flash and exit edit mode
                    timezone_field.set_offset_minutes(offset_minutes)?;
                    info!("Offset saved to flash: {} minutes", offset_minutes);
                    return Ok(Self::HoursMinutes { speed: 1.0 });
                }
            }
        }
    }
}

struct Led12x4ClockDisplay {
    display: Led12x4<LedStripSimple<'static, embassy_rp::peripherals::PIO1, 48>>,
}

impl Led12x4ClockDisplay {
    fn new(display: Led12x4<LedStripSimple<'static, embassy_rp::peripherals::PIO1, 48>>) -> Self {
        Self { display }
    }

    async fn show_portal_ready(&mut self) -> Result<()> {
        self.display
            .display(['0', '0', '0', '0'], [PORTAL_COLOR; 4])
            .await
    }

    async fn show_connecting(&mut self, try_index: u8, try_count: u8) -> Result<()> {
        let attempt = try_index + 1;
        let (attempt_tens, attempt_ones) = two_digit_chars(attempt);
        let (count_tens, count_ones) = two_digit_chars(try_count);
        self.display
            .display(
                [attempt_tens, attempt_ones, count_tens, count_ones],
                [CONNECTING_COLOR; 4],
            )
            .await
    }

    async fn show_hours_minutes(&mut self, hours: u8, minutes: u8) -> Result<()> {
        let (hours_tens, hours_ones) = hours_digits(hours);
        let (minutes_tens, minutes_ones) = two_digit_chars(minutes);
        self.display
            .display(
                [hours_tens, hours_ones, minutes_tens, minutes_ones],
                DIGIT_COLORS,
            )
            .await
    }

    async fn show_hours_minutes_indicator(&mut self, hours: u8, minutes: u8) -> Result<()> {
        let (hours_tens, hours_ones) = hours_digits(hours);
        let (minutes_tens, minutes_ones) = two_digit_chars(minutes);
        self.display
            .display(
                [hours_tens, hours_ones, minutes_tens, minutes_ones],
                EDIT_COLORS,
            )
            .await
    }

    async fn show_minutes_seconds(&mut self, minutes: u8, seconds: u8) -> Result<()> {
        let (minutes_tens, minutes_ones) = two_digit_chars(minutes);
        let (seconds_tens, seconds_ones) = two_digit_chars(seconds);
        self.display
            .display(
                [minutes_tens, minutes_ones, seconds_tens, seconds_ones],
                DIGIT_COLORS,
            )
            .await
    }
}

#[inline]
fn two_digit_chars(value: u8) -> (char, char) {
    assert!(value < 100);
    (tens_digit(value), ones_digit(value))
}

#[inline]
fn hours_digits(hours: u8) -> (char, char) {
    assert!(hours >= 1 && hours <= 12);
    if hours >= 10 {
        ('1', ones_digit(hours))
    } else {
        (' ', ones_digit(hours))
    }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Value < 100 ensures division is safe"
)]
fn tens_digit(value: u8) -> char {
    ((value / 10) + b'0') as char
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Value < 100 ensures division is safe"
)]
fn ones_digit(value: u8) -> char {
    ((value % 10) + b'0') as char
}
