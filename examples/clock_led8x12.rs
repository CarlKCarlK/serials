//! Wi-Fi enabled 4-digit LED matrix clock (8x12 pixels rotated) with captive-portal setup.
//!
//! This example uses two stacked 12x4 LED panels rotated 90° clockwise to create an 8-wide
//! by 12-tall display. Uses Font4x6Trim for dense 2-line digit display ("12\n34").
//! The panel is on GPIO4, reset button on GPIO13.

#![no_std]
#![no_main]
#![cfg(feature = "wifi")]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use core::pin::pin;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::Duration;
use heapless::String;
use panic_probe as _;
use serials::button::{Button, PressDuration, PressedTo};
use serials::clock::{Clock, ClockStatic, ONE_MINUTE, ONE_SECOND, h12_m_s};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::led_strip_simple::Milliamps;
use serials::led_strip_simple::colors;
use serials::led2d::{Led2dFont, led2d_device_simple};
use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use serials::wifi_setup::fields::{TimezoneField, TimezoneFieldStatic};
use serials::wifi_setup::{WifiSetup, WifiSetupStatic};
use serials::{Error, Result};
use smart_leds::RGB8;

// Rotated display: 8 wide × 12 tall (two 12x4 panels rotated 90° clockwise)
led2d_device_simple! {
    pub led8x12,
    rows: 12,
    cols: 8,
    pio: PIO1,
    mapping: arbitrary([
        47, 46, 45, 44, 95, 94, 93, 92,
        40, 41, 42, 43, 88, 89, 90, 91,
        39, 38, 37, 36, 87, 86, 85, 84,
        32, 33, 34, 35, 80, 81, 82, 83,
        31, 30, 29, 28, 79, 78, 77, 76,
        24, 25, 26, 27, 72, 73, 74, 75,
        23, 22, 21, 20, 71, 70, 69, 68,
        16, 17, 18, 19, 64, 65, 66, 67,
        15, 14, 13, 12, 63, 62, 61, 60,
        8, 9, 10, 11, 56, 57, 58, 59,
        7, 6, 5, 4, 55, 54, 53, 52,
        0, 1, 2, 3, 48, 49, 50, 51,
    ]),
    font: Led2dFont::Font4x6Trim,
}

type LedFrame = serials::led2d::Frame<{ Led8x12::ROWS }, { Led8x12::COLS }>;

const FAST_MODE_SPEED: f32 = 720.0;
const CONNECTING_COLOR: RGB8 = colors::SADDLE_BROWN;
const DIGIT_COLORS: [RGB8; 4] = [colors::CYAN, colors::MAGENTA, colors::ORANGE, colors::LIME];
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

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting Wi-Fi 8x12 LED clock (rotated display)");
    let p = embassy_rp::init(Default::default());

    // Use two blocks of flash storage: Wi-Fi credentials + timezone
    static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
    let [wifi_credentials_flash_block, timezone_flash_block] =
        FlashArray::new(&FLASH_STATIC, p.FLASH)?;

    // Define HTML to ask for timezone on the captive portal.
    static TIMEZONE_FIELD_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
    let timezone_field = TimezoneField::new(&TIMEZONE_FIELD_STATIC, timezone_flash_block);

    // Set up Wifi via a captive portal. The button pin is used to reset stored credentials.
    static WIFI_SETUP_STATIC: WifiSetupStatic = WifiSetup::new_static();
    let wifi_setup = WifiSetup::new(
        &WIFI_SETUP_STATIC,
        p.PIN_23,  // CYW43 power
        p.PIN_25,  // CYW43 chip select
        p.PIO0,    // CYW43 PIO interface
        p.PIN_24,  // CYW43 clock
        p.PIN_29,  // CYW43 data pin
        p.DMA_CH0, // CYW43 DMA channel
        wifi_credentials_flash_block,
        p.PIN_13, // Reset button pin
        PressedTo::Ground,
        "www.picoclock.net", // Captive-portal SSID
        [timezone_field],    // Custom fields to ask for
        spawner,
    )?;

    // Set up the 8x12 LED display on GPIO4.
    static LED_8X12_STATIC: Led8x12Static = Led8x12::new_static();
    let led_8x12 = Led8x12::new(
        &LED_8X12_STATIC,
        p.PIO1,
        p.PIN_4,
        Milliamps(1000), // 1000mA budget for 96 LEDs
        spawner,
    )
    .await?;

    // Connect Wi-Fi, using the LED panel for status.
    let led_8x12_ref = &led_8x12;
    let (stack, mut button) = wifi_setup
        .connect(spawner, move |event| {
            let led_8x12_ref = led_8x12_ref;
            async move {
                use serials::wifi_setup::WifiSetupEvent;
                match event {
                    WifiSetupEvent::CaptivePortalReady => {
                        info!("WiFi: captive portal ready, displaying CONN");
                        show_portal_ready(led_8x12_ref)
                            .await
                            .expect("LED display failed during portal-ready");
                    }
                    WifiSetupEvent::Connecting {
                        try_index,
                        try_count,
                    } => {
                        info!("WiFi: connecting (attempt {}/{})", try_index + 1, try_count);
                        show_connecting(led_8x12_ref, try_index, try_count)
                            .await
                            .expect("LED display failed during connecting");
                    }
                    WifiSetupEvent::Connected => {
                        info!("WiFi: connected successfully, displaying DONE");
                        show_connected(led_8x12_ref)
                            .await
                            .expect("LED display failed during connected");
                    }
                    WifiSetupEvent::ConnectionFailed => {
                        info!("WiFi: connection failed, displaying FAIL, device will reset");
                        show_connection_failed(led_8x12_ref)
                            .await
                            .expect("LED display failed during connection-failed");
                    }
                }
            }
        })
        .await?;

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
                    .execute_hours_minutes(speed, &clock, &mut button, &time_sync, &led_8x12)
                    .await?
            }
            State::MinutesSeconds => {
                state
                    .execute_minutes_seconds(&clock, &mut button, &time_sync, &led_8x12)
                    .await?
            }
            State::EditOffset => {
                state
                    .execute_edit_offset(&clock, &mut button, &timezone_field, &led_8x12)
                    .await?
            }
        };
    }
}

// State machine for 8x12 LED clock display modes and transitions.

/// Display states for the 8x12 LED clock.
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
        led_8x12: &Led8x12,
    ) -> Result<Self> {
        clock.set_speed(speed).await;
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        show_hours_minutes(led_8x12, hours, minutes).await?;
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
                    show_hours_minutes(led_8x12, hours, minutes).await?;
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
        led_8x12: &Led8x12,
    ) -> Result<Self> {
        clock.set_speed(1.0).await;
        let (_, minutes, seconds) = h12_m_s(&clock.now_local());
        show_minutes_seconds(led_8x12, minutes, seconds).await?;
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
                    show_minutes_seconds(led_8x12, minutes, seconds).await?;
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
        led_8x12: &Led8x12,
    ) -> Result<Self> {
        info!("Entering edit offset mode");
        clock.set_speed(1.0).await;

        // Blink current hours and minutes with edit color accent.
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        show_hours_minutes_indicator(led_8x12, hours, minutes).await?;

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

                    // Update display
                    let (hours, minutes, _) = h12_m_s(&clock.now_local());
                    info!(
                        "Updated time after offset change: {:02}:{:02}",
                        hours, minutes
                    );
                    show_hours_minutes_indicator(led_8x12, hours, minutes).await?;
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

// Display helper functions for the 8x12 LED clock

async fn show_portal_ready(led_8x12: &Led8x12) -> Result<()> {
    let on_frame = text_frame(led_8x12, "CO\nNN", &DIGIT_COLORS)?;
    led_8x12
        .animate(&[
            (on_frame, Duration::from_millis(700)),
            (Led8x12::new_frame(), Duration::from_millis(300)),
        ])
        .await
}

async fn show_connecting(led_8x12: &Led8x12, try_index: u8, _try_count: u8) -> Result<()> {
    let clockwise = try_index % 2 == 0;
    const FRAME_DURATION: Duration = Duration::from_millis(90);
    let animation = perimeter_chase_animation(clockwise, CONNECTING_COLOR, FRAME_DURATION)?;
    led_8x12.animate(&animation).await
}

async fn show_connected(led_8x12: &Led8x12) -> Result<()> {
    led_8x12.write_text("DO\nNE", &DIGIT_COLORS).await
}

async fn show_connection_failed(led_8x12: &Led8x12) -> Result<()> {
    led_8x12.write_text("FA\nIL", &DIGIT_COLORS).await
}

async fn show_hours_minutes(led_8x12: &Led8x12, hours: u8, minutes: u8) -> Result<()> {
    let (hours_tens, hours_ones) = hours_digits(hours);
    let (minutes_tens, minutes_ones) = two_digit_chars(minutes);
    let text = two_line_text([hours_tens, hours_ones], [minutes_tens, minutes_ones]);
    led_8x12.write_text(text.as_str(), &DIGIT_COLORS).await
}

async fn show_hours_minutes_indicator(led_8x12: &Led8x12, hours: u8, minutes: u8) -> Result<()> {
    let (hours_tens, hours_ones) = hours_digits(hours);
    let (minutes_tens, minutes_ones) = two_digit_chars(minutes);
    let text = two_line_text([hours_tens, hours_ones], [minutes_tens, minutes_ones]);
    led_8x12.write_text(text.as_str(), &EDIT_COLORS).await
}

async fn show_minutes_seconds(led_8x12: &Led8x12, minutes: u8, seconds: u8) -> Result<()> {
    let (minutes_tens, minutes_ones) = two_digit_chars(minutes);
    let (seconds_tens, seconds_ones) = two_digit_chars(seconds);
    let text = two_line_text([minutes_tens, minutes_ones], [seconds_tens, seconds_ones]);
    led_8x12.write_text(text.as_str(), &DIGIT_COLORS).await
}

const PERIMETER_LENGTH: usize = (Led8x12::COLS * 2) + ((Led8x12::ROWS - 2) * 2);

fn two_line_text(top_chars: [char; 2], bottom_chars: [char; 2]) -> String<5> {
    let mut text = String::new();
    for ch in top_chars {
        text.push(ch).expect("text buffer has capacity");
    }
    text.push('\n').expect("text buffer has capacity");
    for ch in bottom_chars {
        text.push(ch).expect("text buffer has capacity");
    }
    text
}

fn text_frame(led_8x12: &Led8x12, text: &str, colors: &[RGB8]) -> Result<LedFrame> {
    let mut frame = Led8x12::new_frame();
    led_8x12.write_text_to_frame(text, colors, &mut frame)?;
    Ok(frame)
}

fn perimeter_chase_animation(
    clockwise: bool,
    color: RGB8,
    duration: Duration,
) -> Result<heapless::Vec<(LedFrame, Duration), PERIMETER_LENGTH>> {
    assert!(
        duration.as_micros() > 0,
        "perimeter animation duration must be positive"
    );
    let coordinates = perimeter_coordinates(clockwise);
    let mut frames = heapless::Vec::new();
    for frame_index in 0..PERIMETER_LENGTH {
        let mut frame = Led8x12::new_frame();
        let (row_index, column_index) = coordinates[frame_index];
        frame[row_index][column_index] = color;
        frames
            .push((frame, duration))
            .map_err(|_| Error::FormatError)?;
    }
    Ok(frames)
}

fn perimeter_coordinates(clockwise: bool) -> [(usize, usize); PERIMETER_LENGTH] {
    let mut coordinates = [(0_usize, 0_usize); PERIMETER_LENGTH];
    let mut write_index = 0;
    let mut push = |row_index: usize, column_index: usize| {
        coordinates[write_index] = (row_index, column_index);
        write_index += 1;
    };

    for column_index in 0..Led8x12::COLS {
        push(0, column_index);
    }
    for row_index in 1..Led8x12::ROWS {
        push(row_index, Led8x12::COLS - 1);
    }
    for column_index in (0..(Led8x12::COLS - 1)).rev() {
        push(Led8x12::ROWS - 1, column_index);
    }
    for row_index in (1..(Led8x12::ROWS - 1)).rev() {
        push(row_index, 0);
    }

    debug_assert_eq!(write_index, PERIMETER_LENGTH);

    if clockwise {
        coordinates
    } else {
        let mut reversed = [(0_usize, 0_usize); PERIMETER_LENGTH];
        for (reverse_index, &(row_index, column_index)) in coordinates.iter().enumerate() {
            reversed[PERIMETER_LENGTH - 1 - reverse_index] = (row_index, column_index);
        }
        reversed
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
