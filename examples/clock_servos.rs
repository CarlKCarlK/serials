//! Wi-Fi enabled clock that visualizes time with two servos.
//!
//! This example combines the `WifiSetup` captive-portal workflow with a servo-based
//! display. Because the servos are mounted reversed, the left servo shows minutes/seconds
//! and the right servo shows hours/minutes with 180° reflections applied.

#![no_std]
#![no_main]
#![cfg(feature = "wifi")]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use panic_probe as _;
use serials::button::{Button, PressDuration};
use serials::clock::{Clock, ClockStatic, ONE_MINUTE, ONE_SECOND, h12_m_s};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::servo::servo_even;
use serials::servo_wiggle::{WiggleMode, WigglingServo, WigglingServoStatic};
use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use serials::wifi_setup::fields::{TimezoneField, TimezoneFieldStatic};
use serials::wifi_setup::{WifiSetup, WifiSetupStatic};
use serials::{Error, Result};

const FAST_MODE_SPEED: f32 = 720.0;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("Starting Wi-Fi servo clock (WifiSetup)");
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
        peripherals.PIN_13, // Reset button pin
        "PicoServoClock",   // Captive-portal SSID
        [timezone_field],
        spawner,
    )?;

    // Configure two servos for the display.
    static LEFT_SERVO_WIGGLE_STATIC: WigglingServoStatic = WigglingServo::new_static();
    static RIGHT_SERVO_WIGGLE_STATIC: WigglingServoStatic = WigglingServo::new_static();
    let servo_display = ServoClockDisplay::new(
        WigglingServo::new(
            &LEFT_SERVO_WIGGLE_STATIC,
            servo_even!(peripherals.PIN_0, peripherals.PWM_SLICE0, 500, 2500),
            spawner,
        )?,
        WigglingServo::new(
            &RIGHT_SERVO_WIGGLE_STATIC,
            servo_even!(peripherals.PIN_2, peripherals.PWM_SLICE1, 500, 2500),
            spawner,
        )?,
    );

    // Connect Wi-Fi, using the servos for status indications.
    let servo_display_ref = &servo_display;
    let (stack, mut button) = wifi_setup
        .connect(spawner, move |event| {
            let servo_display_ref = servo_display_ref;
            async move {
                use serials::wifi_setup::WifiSetupEvent;
                match event {
                    WifiSetupEvent::CaptivePortalReady => servo_display_ref.show_portal_ready().await,
                    WifiSetupEvent::Connecting { .. } => servo_display_ref.show_connecting().await,
                    WifiSetupEvent::Connected => servo_display_ref.show_connected().await,
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
    let mut state = State::HoursMinutes;
    loop {
        state = match state {
            State::HoursMinutes => {
                state
                    .execute_hours_minutes(&clock, &mut button, &time_sync, &servo_display)
                    .await?
            }
            State::MinutesSeconds => {
                state
                    .execute_minutes_seconds(&clock, &mut button, &time_sync, &servo_display)
                    .await?
            }
            State::HoursMinutesFast => {
                state
                    .execute_hours_minutes_fast(
                        &clock,
                        &mut button,
                        &time_sync,
                        &servo_display,
                    )
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
    HoursMinutes,
    MinutesSeconds,
    HoursMinutesFast,
    EditOffset,
}

impl State {
    async fn execute_hours_minutes(
        self,
        clock: &Clock,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        servo_display: &ServoClockDisplay,
    ) -> Result<Self> {
        clock.set_speed(1.0).await;
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        servo_display.show_hours_minutes(hours, minutes).await;
        clock.set_tick_interval(Some(ONE_MINUTE)).await;
        loop {
            match select(
                select(button.press_duration(), clock.wait()),
                time_sync.wait(),
            )
            .await
            {
                // Button pushes
                Either::First(Either::First(PressDuration::Short)) => {
                    return Ok(Self::MinutesSeconds);
                }
                Either::First(Either::First(PressDuration::Long)) => {
                    return Ok(Self::EditOffset);
                }
                // Clock tick
                Either::First(Either::Second(time_event)) => {
                    let (hours, minutes, _) = h12_m_s(&time_event);
                    servo_display.show_hours_minutes(hours, minutes).await;
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
                select(button.press_duration(), clock.wait()),
                time_sync.wait(),
            )
            .await
            {
                // Button pushes
                Either::First(Either::First(PressDuration::Short)) => {
                    return Ok(Self::HoursMinutesFast);
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

    async fn execute_hours_minutes_fast(
        self,
        clock: &Clock,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        servo_display: &ServoClockDisplay,
    ) -> Result<Self> {
        clock.set_speed(FAST_MODE_SPEED).await;
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        servo_display.show_hours_minutes(hours, minutes).await;
        clock.set_tick_interval(Some(ONE_MINUTE)).await;
        let display_task = async {
            loop {
                match select(clock.wait(), time_sync.wait()).await {
                    Either::First(time_event) => {
                        let (hours, minutes, _) = h12_m_s(&time_event);
                        servo_display.show_hours_minutes(hours, minutes).await;
                    }
                    Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                        info!(
                            "Time sync success: setting clock to {}",
                            unix_seconds.as_i64()
                        );
                        clock.set_utc_time(unix_seconds).await;
                        let (hours, minutes, _) = h12_m_s(&clock.now_local());
                        servo_display.show_hours_minutes(hours, minutes).await;
                    }
                    Either::Second(TimeSyncEvent::Failed(msg)) => {
                        info!("Time sync failed: {}", msg);
                    }
                }
            }
        };

        match select(button.press_duration(), display_task).await {
            Either::First(PressDuration::Short) => {
                clock.set_speed(1.0).await;
                Ok(Self::HoursMinutes)
            }
            Either::First(PressDuration::Long) => {
                clock.set_speed(1.0).await;
                Ok(Self::EditOffset)
            }
            Either::Second(_) => {
                // display_task never completes
                unreachable!()
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

        // Get the current offset minutes from clock (source of truth)
        let mut offset_minutes = clock.offset_minutes();
        info!("Current offset: {} minutes", offset_minutes);

        clock.set_tick_interval(None).await; // Disable ticks in edit mode
        loop {
            info!("Waiting for button press in edit mode");
            match button.press_duration().await {
                PressDuration::Short => {
                    info!("Short press detected - incrementing offset");
                    // Increment the offset by 1 hour
                    offset_minutes += 60;
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
                }
                PressDuration::Long => {
                    info!("Long press detected - saving and exiting edit mode");
                    // Save to flash and exit edit mode
                    timezone_field.set_offset_minutes(offset_minutes)?;
                    info!("Offset saved to flash: {} minutes", offset_minutes);
                    return Ok(Self::HoursMinutes);
                }
            }
        }
    }
}

struct ServoClockDisplay {
    left: WigglingServo,
    right: WigglingServo,
}

impl ServoClockDisplay {
    fn new(left: WigglingServo, right: WigglingServo) -> Self {
        Self { left, right }
    }

    async fn show_portal_ready(&self) {
        self.set_angles(0, WiggleMode::Still, 0, WiggleMode::Still)
            .await;
    }

    async fn show_connecting(&self) {
        self.set_angles(90, WiggleMode::Still, 90, WiggleMode::Still)
            .await;
    }

    async fn show_connected(&self) {
        self.set_angles(180, WiggleMode::Still, 180, WiggleMode::Still)
            .await;
    }

    async fn show_hours_minutes(&self, hours: u8, minutes: u8) {
        let left_angle = hours_to_degrees(hours);
        let right_angle = sixty_to_degrees(minutes);
        self.set_angles(left_angle, WiggleMode::Still, right_angle, WiggleMode::Still)
            .await;
    }

    async fn show_hours_minutes_indicator(&self, hours: u8, minutes: u8) {
        let left_angle = hours_to_degrees(hours);
        let right_angle = sixty_to_degrees(minutes);
        self.set_angles(left_angle, WiggleMode::Still, right_angle, WiggleMode::Wiggle)
            .await;
    }

    async fn show_minutes_seconds(&self, minutes: u8, seconds: u8) {
        let left_angle = sixty_to_degrees(minutes);
        let right_angle = sixty_to_degrees(seconds);
        self.set_angles(left_angle, WiggleMode::Still, right_angle, WiggleMode::Still)
            .await;
    }

    async fn set_angles(
        &self,
        left_degrees: i32,
        left_mode: WiggleMode,
        right_degrees: i32,
        right_mode: WiggleMode,
    ) {
        // Swap servos and reflect angles for physical orientation.
        let physical_left = reflect_degrees(right_degrees);
        let physical_right = reflect_degrees(left_degrees);
        self.left.set(physical_left, right_mode).await;
        self.right.set(physical_right, left_mode).await;
    }
}

#[inline]
fn hours_to_degrees(hours: u8) -> i32 {
    assert!((1..=12).contains(&hours));
    i32::from(hours) * 180 / 12
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
