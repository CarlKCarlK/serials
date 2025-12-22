//! Wi-Fi enabled clock that visualizes time with two servos.
//!
//! This example combines the `WifiAuto` captive-portal workflow with a servo-based
//! display. Because the servos are mounted reversed, the left servo shows minutes/seconds
//! and the right servo shows hours/minutes with 180° reflections applied.

#![no_std]
#![no_main]
#![cfg(feature = "wifi")]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::{
    convert::{Infallible, TryFrom},
    pin::pin,
};
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::Duration;
use panic_probe as _;
use device_kit::button::{Button, PressDuration, PressedTo};
use device_kit::clock::{Clock, ClockStatic, ONE_MINUTE, ONE_SECOND, h12_m_s};
use device_kit::flash_array::{FlashArray, FlashArrayStatic};
use device_kit::servo_animate::{ServoAnimate, ServoAnimateStatic, Step, linear, servo_even};
use device_kit::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use device_kit::wifi_auto::fields::{TimezoneField, TimezoneFieldStatic};
use device_kit::wifi_auto::{WifiAuto, WifiAutoStatic};
use device_kit::{Error, Result};

const FAST_MODE_SPEED: f32 = 720.0;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting Wi-Fi servo clock (WifiAuto)");
    let p = embassy_rp::init(Default::default());

    // Use two blocks of flash storage: Wi-Fi credentials + timezone
    static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
    let [wifi_credentials_flash_block, timezone_flash_block] =
        FlashArray::new(&FLASH_STATIC, p.FLASH)?;

    // Define HTML to ask for timezone on the captive portal.
    static TIMEZONE_FIELD_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
    let timezone_field = TimezoneField::new(&TIMEZONE_FIELD_STATIC, timezone_flash_block);

    // Set up Wifi via a captive portal. The button pin is used to reset stored credentials.
    static WIFI_AUTO_STATIC: WifiAutoStatic = WifiAuto::new_static();
    let wifi_auto = WifiAuto::new(
        &WIFI_AUTO_STATIC,
        p.PIN_23,  // CYW43 power
        p.PIN_25,  // CYW43 chip select
        p.PIO0,    // CYW43 PIO interface
        p.PIN_24,  // CYW43 clock
        p.PIN_29,  // CYW43 data pin
        p.DMA_CH0, // CYW43 DMA channel
        wifi_credentials_flash_block,
        p.PIN_13, // Reset button pin
        PressedTo::Ground,
        "PicoServoClock", // Captive-portal SSID
        [timezone_field],
        spawner,
    )?;

    // Configure two servos for the display.
    static LEFT_SERVO_ANIMATE_STATIC: ServoAnimateStatic = ServoAnimate::new_static();
    static RIGHT_SERVO_ANIMATE_STATIC: ServoAnimateStatic = ServoAnimate::new_static();
    let servo_display = ServoClockDisplay::new(
        ServoAnimate::new(
            &LEFT_SERVO_ANIMATE_STATIC,
            servo_even!(p.PIN_0, p.PWM_SLICE0, 500, 2500),
            spawner,
        )?,
        ServoAnimate::new(
            &RIGHT_SERVO_ANIMATE_STATIC,
            servo_even!(p.PIN_2, p.PWM_SLICE1, 500, 2500),
            spawner,
        )?,
    );

    // Connect Wi-Fi, using the servos for status indications.
    let servo_display_ref = &servo_display;
    let (stack, mut button) = wifi_auto
        .connect(spawner, move |event| {
            let servo_display_ref = servo_display_ref;
            async move {
                use device_kit::wifi_auto::WifiAutoEvent;
                match event {
                    WifiAutoEvent::CaptivePortalReady => {
                        servo_display_ref.show_portal_ready().await
                    }
                    WifiAutoEvent::Connecting { .. } => servo_display_ref.show_connecting().await,
                    WifiAutoEvent::Connected => {
                        // No-op; main loop will immediately render real time.
                    }
                    WifiAutoEvent::ConnectionFailed => {
                        // No-op; portal remains visible on failure.
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
                    .execute_hours_minutes(speed, &clock, &mut button, &time_sync, &servo_display)
                    .await?
            }
            State::MinutesSeconds => {
                state
                    .execute_minutes_seconds(&clock, &mut button, &time_sync, &servo_display)
                    .await?
            }
            State::EditOffset => {
                state
                    .execute_edit_offset(&clock, &mut button, &timezone_field, &servo_display)
                    .await?
            }
        };
    }
}

// State machine for servo clock display modes and transitions.

/// Display states for the servo clock.
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
        servo_display: &ServoClockDisplay,
    ) -> Result<Self> {
        clock.set_speed(speed).await;
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        servo_display.show_hours_minutes(hours, minutes).await;
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
                Either::First(press_duration) => match (press_duration, speed.to_bits()) {
                    (PressDuration::Short, bits) if bits == 1.0f32.to_bits() => {
                        return Ok(Self::MinutesSeconds);
                    }
                    (PressDuration::Short, _) => {
                        return Ok(Self::HoursMinutes { speed: 1.0 });
                    }
                    (PressDuration::Long, _) => {
                        return Ok(Self::EditOffset);
                    }
                },
                // Clock tick
                Either::Second(Either::First(time_event)) => {
                    let (hours, minutes, _) = h12_m_s(&time_event);
                    servo_display.show_hours_minutes(hours, minutes).await;
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
        servo_display: &ServoClockDisplay,
    ) -> Result<Self> {
        clock.set_speed(1.0).await;
        let (_, minutes, seconds) = h12_m_s(&clock.now_local());
        servo_display.show_minutes_seconds(minutes, seconds).await;
        clock.set_tick_interval(Some(ONE_SECOND)).await;
        loop {
            match select(
                select(button.wait_for_press_duration(), clock.wait_for_tick()),
                time_sync.wait_for_sync(),
            )
            .await
            {
                // Button pushes
                Either::First(Either::First(PressDuration::Short)) => {
                    return Ok(Self::HoursMinutes {
                        speed: FAST_MODE_SPEED,
                    });
                }
                Either::First(Either::First(PressDuration::Long)) => {
                    return Ok(Self::EditOffset);
                }
                // Clock tick
                Either::First(Either::Second(time_event)) => {
                    let (_, minutes, seconds) = h12_m_s(&time_event);
                    servo_display.show_minutes_seconds(minutes, seconds).await;
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
        servo_display: &ServoClockDisplay,
    ) -> Result<Self> {
        info!("Entering edit offset mode");
        clock.set_speed(1.0).await;

        // Show current hours and minutes
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        servo_display
            .show_hours_minutes_indicator(hours, minutes)
            .await;
        // Add a gentle wiggle on the bottom servo to signal edit mode.
        const WIGGLE: [Step; 2] = [
            Step {
                degrees: 80,
                duration: Duration::from_millis(250),
            },
            Step {
                degrees: 100,
                duration: Duration::from_millis(250),
            },
        ];
        servo_display.bottom.animate(&WIGGLE).await;

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

                    // Update display (atomic already updated, can use now_local)
                    let (hours, minutes, _) = h12_m_s(&clock.now_local());
                    info!(
                        "Updated time after offset change: {:02}:{:02}",
                        hours, minutes
                    );
                    servo_display
                        .show_hours_minutes_indicator(hours, minutes)
                        .await;
                    servo_display.bottom.animate(&WIGGLE).await;
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

struct ServoClockDisplay {
    bottom: ServoAnimate,
    top: ServoAnimate,
}

impl ServoClockDisplay {
    fn new(bottom: ServoAnimate, top: ServoAnimate) -> Self {
        Self { bottom, top }
    }

    async fn show_portal_ready(&self) {
        self.bottom.set(90).await;
        self.top.set(90).await;
    }

    async fn show_connecting(&self) {
        // Keep bottom servo fixed; animate top servo through a two-phase sweep.
        self.bottom.set(0).await;
        // cmk understand if we really want this to have 11 steps and a sleep after each.
        const FIVE_SECONDS: Duration = Duration::from_secs(5);
        let clockwise = linear::<10>(180 - 18, 0, FIVE_SECONDS);
        let and_back = linear::<2>(0, 180, FIVE_SECONDS);
        let top_sequence = device_kit::servo_animate::concat_steps::<16>(&[&clockwise, &and_back]);
        self.top.animate(&top_sequence).await;
        let bottom_sequence = device_kit::servo_animate::concat_steps::<16>(&[&and_back, &clockwise]);
        self.bottom.animate(&bottom_sequence).await;
    }

    async fn show_hours_minutes(&self, hours: u8, minutes: u8) {
        let left_angle = hours_to_degrees(hours);
        let right_angle = sixty_to_degrees(minutes);
        self.set_angles(left_angle, right_angle).await;
    }

    async fn show_hours_minutes_indicator(&self, hours: u8, minutes: u8) {
        let left_angle = hours_to_degrees(hours);
        let right_angle = sixty_to_degrees(minutes);
        self.set_angles(left_angle, right_angle).await;
    }

    async fn show_minutes_seconds(&self, minutes: u8, seconds: u8) {
        let left_angle = sixty_to_degrees(minutes);
        let right_angle = sixty_to_degrees(seconds);
        self.set_angles(left_angle, right_angle).await;
    }

    async fn set_angles(&self, left_degrees: i32, right_degrees: i32) {
        // Swap servos and reflect angles for physical orientation.
        let physical_left = reflect_degrees(right_degrees);
        let physical_right = reflect_degrees(left_degrees);
        let left_angle =
            u16::try_from(physical_left).expect("servo angles must be between 0 and 180 degrees");
        let right_angle =
            u16::try_from(physical_right).expect("servo angles must be between 0 and 180 degrees");
        self.bottom.set(left_angle).await;
        self.top.set(right_angle).await;
    }
}

#[inline]
fn hours_to_degrees(hours: u8) -> i32 {
    assert!((1..=12).contains(&hours));
    let normalized_hour = hours % 12;
    i32::from(normalized_hour) * 180 / 12
}

#[inline]
fn sixty_to_degrees(value: u8) -> i32 {
    assert!(value < 60);
    i32::from(value) * 180 / 60
}

#[inline]
fn reflect_degrees(degrees: i32) -> i32 {
    assert!((0..=180).contains(&degrees));
    180 - degrees
}
