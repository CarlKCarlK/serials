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
use device_kit::button::{Button, PressDuration, PressedTo};
use device_kit::clock::{Clock, ClockStatic, ONE_MINUTE, ONE_SECOND, h12_m_s};
use device_kit::flash_array::{FlashArray, FlashArrayStatic};
use device_kit::led_strip_simple::Milliamps;
use device_kit::led_strip_simple::colors;
use device_kit::led2d::led2d_device_simple;
use device_kit::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use device_kit::wifi_setup::fields::{TimezoneField, TimezoneFieldStatic};
use device_kit::wifi_setup::{WifiSetup, WifiSetupStatic};
use device_kit::{Error, Result};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::Duration;
use heapless::String;
use micromath::F32Ext;
use panic_probe as _;
use smart_leds::RGB8;

// cmk could/should we replace arbitrary with a cat of the zigzag mapping?
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
    max_frames: 48,
    font: Font4x6Trim,
}

type LedFrame = device_kit::led2d::Frame<{ Led8x12::ROWS }, { Led8x12::COLS }>;

const FAST_MODE_SPEED: f32 = 720.0;
const CONNECTING_COLOR: RGB8 = colors::SADDLE_BROWN;
const DIGIT_COLORS: [RGB8; 4] = [colors::CYAN, colors::MAGENTA, colors::ORANGE, colors::LIME];
const EDIT_COLORS: [RGB8; 4] = [
    colors::FIREBRICK,
    colors::DARK_ORANGE,
    colors::TEAL,
    colors::MAROON,
];
const ANALOG_MINUTE_HAND_COLOR: RGB8 = colors::CYAN;
const ANALOG_HOUR_HAND_COLOR: RGB8 = colors::ORANGE;
const ANALOG_MINUTE_HAND_LENGTH: f32 = 3.5;
const ANALOG_HOUR_HAND_LENGTH: f32 = 2.5;

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
        Milliamps(250), // 1000mA budget for 96 LEDs
        spawner,
    )
    .await?;

    // Connect Wi-Fi, using the LED panel for status.
    let led_8x12_ref = &led_8x12;
    let (stack, mut button) = wifi_setup
        .connect(spawner, move |event| {
            let led_8x12_ref = led_8x12_ref;
            async move {
                use device_kit::wifi_setup::WifiSetupEvent;
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
            State::Analog { speed } => {
                state
                    .execute_analog(speed, &clock, &mut button, &time_sync, &led_8x12)
                    .await?
            }
            State::ClockDots { speed } => {
                state
                    .execute_clock_dots(speed, &clock, &mut button, &time_sync, &led_8x12)
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
    Analog { speed: f32 },
    ClockDots { speed: f32 },
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
                            info!("HoursMinutes -> Analog");
                            return Ok(Self::Analog { speed: 1.0 });
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
                            info!("MinutesSeconds -> Analog (fast)");
                            return Ok(Self::Analog {
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

    async fn execute_analog(
        self,
        speed: f32,
        clock: &Clock,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        led_8x12: &Led8x12,
    ) -> Result<Self> {
        clock.set_speed(speed).await;
        let (hours, minutes, seconds) = h12_m_s(&clock.now_local());
        show_analog(led_8x12, hours, minutes, seconds).await?;
        clock.set_tick_interval(Some(ONE_SECOND)).await;
        let mut button_press = pin!(button.wait_for_press_duration());
        loop {
            match select(
                &mut button_press,
                select(clock.wait_for_tick(), time_sync.wait_for_sync()),
            )
            .await
            {
                Either::First(press_duration) => {
                    info!(
                        "Analog: Button press detected: {:?}, speed_bits={}",
                        press_duration,
                        speed.to_bits()
                    );
                    match (press_duration, speed.to_bits()) {
                        (PressDuration::Short, bits) if bits == 1.0f32.to_bits() => {
                            info!("Analog -> ClockDots");
                            return Ok(Self::ClockDots { speed: 1.0 });
                        }
                        (PressDuration::Short, _) => {
                            info!("Analog: Resetting speed to 1.0");
                            return Ok(Self::Analog { speed: 1.0 });
                        }
                        (PressDuration::Long, _) => {
                            info!("Analog -> EditOffset");
                            return Ok(Self::EditOffset);
                        }
                    }
                }
                Either::Second(Either::First(time_event)) => {
                    let (hours, minutes, seconds) = h12_m_s(&time_event);
                    show_analog(led_8x12, hours, minutes, seconds).await?;
                }
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

    async fn execute_clock_dots(
        self,
        speed: f32,
        clock: &Clock,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        led_8x12: &Led8x12,
    ) -> Result<Self> {
        clock.set_speed(speed).await;
        let (hours, minutes, seconds) = h12_m_s(&clock.now_local());
        show_clock_dots(led_8x12, hours, minutes, seconds).await?;
        clock.set_tick_interval(Some(ONE_SECOND)).await;
        let mut button_press = pin!(button.wait_for_press_duration());
        loop {
            match select(
                &mut button_press,
                select(clock.wait_for_tick(), time_sync.wait_for_sync()),
            )
            .await
            {
                Either::First(press_duration) => {
                    info!(
                        "ClockDots: Button press detected: {:?}, speed_bits={}",
                        press_duration,
                        speed.to_bits()
                    );
                    match (press_duration, speed.to_bits()) {
                        (PressDuration::Short, bits) if bits == 1.0f32.to_bits() => {
                            info!("ClockDots -> MinutesSeconds");
                            return Ok(Self::MinutesSeconds);
                        }
                        (PressDuration::Short, _) => {
                            info!("ClockDots: Resetting speed to 1.0");
                            return Ok(Self::ClockDots { speed: 1.0 });
                        }
                        (PressDuration::Long, _) => {
                            info!("ClockDots -> EditOffset");
                            return Ok(Self::EditOffset);
                        }
                    }
                }
                Either::Second(Either::First(time_event)) => {
                    let (hours, minutes, seconds) = h12_m_s(&time_event);
                    show_clock_dots(led_8x12, hours, minutes, seconds).await?;
                }
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
                    const ONE_DAY_MINUTES: i32 = device_kit::clock::ONE_DAY.as_secs() as i32 / 60;
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
    // Delay animation start to avoid wifi initialization glitches
    embassy_time::Timer::after(Duration::from_secs(1)).await;

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

async fn show_analog(led_8x12: &Led8x12, hours: u8, minutes: u8, seconds: u8) -> Result<()> {
    use device_kit::led2d::rgb8_to_rgb888;

    let mut frame = Led8x12::new_frame();

    // Draw minute hand first (longer)
    draw_hand(
        &mut frame,
        analog_minute_progress(minutes, seconds),
        ANALOG_MINUTE_HAND_LENGTH,
        rgb8_to_rgb888(ANALOG_MINUTE_HAND_COLOR),
    )?;

    // Draw hour hand second (shorter, on top)
    draw_hand(
        &mut frame,
        analog_hour_progress(hours, minutes, seconds),
        ANALOG_HOUR_HAND_LENGTH,
        rgb8_to_rgb888(ANALOG_HOUR_HAND_COLOR),
    )?;

    led_8x12.write_frame(frame).await
}

async fn show_clock_dots(led_8x12: &Led8x12, hours: u8, minutes: u8, _seconds: u8) -> Result<()> {
    use device_kit::led2d::rgb8_to_rgb888;
    use embedded_graphics::{
        Drawable,
        pixelcolor::Rgb888,
        prelude::*,
        primitives::{PrimitiveStyle, Rectangle},
    };

    let mut frame = Led8x12::new_frame();

    // Draw green 7x7 square background (centered in 8-wide display)
    // Center at column 3.5 (use cols 0-6, leaving 1 pixel on right)
    // Center vertically in 12-row display: use rows 2-8 (leaving 2 at top, 4 at bottom) for symmetry
    let square_top_left = Point::new(0, 2);
    let square_size = Size::new(7, 7);
    Rectangle::new(square_top_left, square_size)
        .into_styled(PrimitiveStyle::with_fill(Rgb888::GREEN))
        .draw(&mut frame)?;

    // Draw 12 white dots for clock positions
    let minute_position = (minutes as f32 / 60.0 * 12.0).round() as u8 % 12;
    let hour_position = hours % 12;

    for position in 0..12 {
        let progress = position as f32 / 12.0;
        let (row, col) = clock_dot_position(progress);

        let color = if position == minute_position {
            Rgb888::BLUE
        } else if position == hour_position {
            rgb8_to_rgb888(colors::ORANGE)
        } else {
            Rgb888::WHITE
        };

        frame[row][col] = device_kit::led2d::rgb888_to_rgb8(color);
    }

    led_8x12.write_frame(frame).await
}

fn clock_dot_position(progress: f32) -> (usize, usize) {
    // Position dots around perimeter of 7x7 square (rows 2-8, cols 0-6)
    // 12 positions mapped to square perimeter:
    // Top row: 11, 12, 1
    // Right column: 2, 3, 4
    // Bottom row: 5, 6, 7
    // Left column: 8, 9, 10

    let position = (progress * 12.0).round() as u8 % 12;

    let (row, col) = match position {
        0 => (2, 3),  // 12 - top center
        1 => (2, 5),  // 1 - top right
        2 => (3, 6),  // 2 - right side upper
        3 => (5, 6),  // 3 - right side middle
        4 => (7, 6),  // 4 - right side lower
        5 => (8, 5),  // 5 - bottom right
        6 => (8, 3),  // 6 - bottom center
        7 => (8, 1),  // 7 - bottom left
        8 => (7, 0),  // 8 - left side lower
        9 => (5, 0),  // 9 - left side middle
        10 => (3, 0), // 10 - left side upper
        11 => (2, 1), // 11 - top left
        _ => (5, 3),  // Fallback to center (unreachable)
    };

    (row, col)
}

fn hour_hand_position(progress: f32) -> (usize, usize) {
    // Position hour hand endpoints pulled in 1 pixel from perimeter
    // Same 12 positions as minute hand but closer to center

    let position = (progress * 12.0).round() as u8 % 12;

    let (row, col) = match position {
        0 => (3, 3),  // 12 - top center (pulled in from 2,3)
        1 => (3, 5),  // 1 - top right (pulled in from 2,5)
        2 => (3, 5),  // 2 - right side upper (pulled in from 3,6)
        3 => (5, 5),  // 3 - right side middle (pulled in from 5,6)
        4 => (7, 5),  // 4 - right side lower (pulled in from 7,6)
        5 => (7, 5),  // 5 - bottom right (pulled in from 8,5)
        6 => (7, 3),  // 6 - bottom center (pulled in from 8,3)
        7 => (7, 1),  // 7 - bottom left (pulled in from 8,1)
        8 => (7, 1),  // 8 - left side lower (pulled in from 7,0)
        9 => (5, 1),  // 9 - left side middle (pulled in from 5,0)
        10 => (3, 1), // 10 - left side upper (pulled in from 3,0)
        11 => (3, 1), // 11 - top left (pulled in from 2,1)
        _ => (5, 3),  // Fallback to center (unreachable)
    };

    (row, col)
}

fn draw_hand(
    frame: &mut LedFrame,
    progress: f32,
    length: f32,
    color: embedded_graphics::pixelcolor::Rgb888,
) -> Result<()> {
    use embedded_graphics::{
        Drawable,
        prelude::*,
        primitives::{Line, PrimitiveStyle},
    };

    assert!(
        progress >= 0.0 && progress <= 1.0,
        "analog hand progress must be within 0.0..=1.0"
    );

    // Center of 7x7 square at rows 2-8, cols 0-6
    let center_row = 5;
    let center_col = 3;

    // Get endpoint position based on hand length
    let (end_row, end_col) = if length >= ANALOG_MINUTE_HAND_LENGTH {
        // Minute hand: use perimeter positions
        clock_dot_position(progress)
    } else {
        // Hour hand: use pulled-in positions
        hour_hand_position(progress)
    };

    let center_point = Point::new(center_col as i32, center_row as i32);
    let end_point = Point::new(end_col as i32, end_row as i32);

    Line::new(center_point, end_point)
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(frame)?;

    Ok(())
}

fn analog_hour_progress(hours: u8, minutes: u8, seconds: u8) -> f32 {
    assert!(
        hours >= 1 && hours <= 12,
        "hours must be 1-12 for analog display"
    );
    assert!(minutes < 60, "minutes must be < 60 for analog display");
    assert!(seconds < 60, "seconds must be < 60 for analog display");
    let hours_mod = if hours == 12 { 0 } else { hours };
    (hours_mod as f32 + (minutes as f32 / 60.0) + (seconds as f32 / 3600.0)) / 12.0
}

fn analog_minute_progress(minutes: u8, seconds: u8) -> f32 {
    assert!(minutes < 60, "minutes must be < 60 for analog display");
    assert!(seconds < 60, "seconds must be < 60 for analog display");
    (minutes as f32 + (seconds as f32 / 60.0)) / 60.0
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
    const SNAKE_LENGTH: usize = 4;
    assert!(
        SNAKE_LENGTH <= PERIMETER_LENGTH,
        "snake length must fit inside the perimeter"
    );
    let coordinates = perimeter_coordinates(clockwise);
    let mut frames = heapless::Vec::new();
    for head_index in 0..PERIMETER_LENGTH {
        let mut frame = Led8x12::new_frame();
        for segment_offset in 0..SNAKE_LENGTH {
            let coordinate_index =
                (head_index + PERIMETER_LENGTH - segment_offset) % PERIMETER_LENGTH;
            let (row_index, column_index) = coordinates[coordinate_index];
            frame[row_index][column_index] = color;
        }
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
    let digit = value / 10;
    if digit == 0 {
        'O'
    } else {
        (digit + b'0') as char
    }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Value < 100 ensures division is safe"
)]
fn ones_digit(value: u8) -> char {
    let digit = value % 10;
    if digit == 0 {
        'O'
    } else {
        (digit + b'0') as char
    }
}
