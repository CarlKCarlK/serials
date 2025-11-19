//! Wi-Fi enabled 4-digit clock that provisions credentials through `WifiAuto`.
//!
//! This example demonstrates how to pair the shared captive-portal workflow with the
//! `ClockLed4` state machine. The `WifiAuto` helper owns Wi-Fi onboarding while the
//! clock display reflects progress and, once connected, continues handling user input.

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::{self, Level};
use embassy_time::{Duration, Timer};
use panic_probe as _;
use serials::Result;
use serials::button::{Button, PressDuration};
use serials::clock_time::{ClockTime, ONE_MINUTE, ONE_SECOND};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::led4::{BlinkState, Led4, Led4Static, OutputArray, circular_outline_animation};
use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use serials::wifi_auto::fields::{TimezoneField, TimezoneFieldStatic};
use serials::wifi_auto::{WifiAuto, WifiAutoStatic};

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("Starting Wi-Fi 4-digit clock (WifiAuto)");
    let peripherals = embassy_rp::init(Default::default());

    // Use two blocks of flash storage: Wi-Fi credentials + timezone
    static FLASH_STATIC: FlashArrayStatic = FlashArray::<2>::new_static();
    let [wifi_credentials_flash_block, timezone_flash_block] =
        FlashArray::new(&FLASH_STATIC, peripherals.FLASH)?;

    // Define HTML to ask for timezone on the captive portal.
    static TIMEZONE_FIELD_STATIC: TimezoneFieldStatic = TimezoneField::new_static();
    let timezone_field = TimezoneField::new(&TIMEZONE_FIELD_STATIC, timezone_flash_block);

    // Set up Wifi via a captive portal. The button pin is used to reset stored credentials.
    // cmk0 think about the WifiAuto name
    static WIFI_AUTO_STATIC: WifiAutoStatic = WifiAuto::new_static();
    let wifi_auto = WifiAuto::new(
        &WIFI_AUTO_STATIC,
        peripherals.PIN_23,  // CYW43 power
        peripherals.PIN_25,  // CYW43 chip select
        peripherals.PIO0,    // CYW43 PIO interface
        peripherals.PIN_24,  // CYW43 clock
        peripherals.PIN_29,  // CYW43 data pin
        peripherals.DMA_CH0, // CYW43 DMA channel
        wifi_credentials_flash_block,
        peripherals.PIN_13, // Reset button pin
        "PicoClock",        // Captive-portal SSID
        [timezone_field],   // Custom fields to ask for
        spawner,
    )?;

    // Initialize LED4 display pins.
    let cell_pins = OutputArray::new([
        gpio::Output::new(peripherals.PIN_1, Level::High),
        gpio::Output::new(peripherals.PIN_2, Level::High),
        gpio::Output::new(peripherals.PIN_3, Level::High),
        gpio::Output::new(peripherals.PIN_4, Level::High),
    ]);

    let segment_pins = OutputArray::new([
        gpio::Output::new(peripherals.PIN_5, Level::Low),
        gpio::Output::new(peripherals.PIN_6, Level::Low),
        gpio::Output::new(peripherals.PIN_7, Level::Low),
        gpio::Output::new(peripherals.PIN_8, Level::Low),
        gpio::Output::new(peripherals.PIN_9, Level::Low),
        gpio::Output::new(peripherals.PIN_10, Level::Low),
        gpio::Output::new(peripherals.PIN_11, Level::Low),
        gpio::Output::new(peripherals.PIN_12, Level::Low),
    ]);

    static LED4_STATIC: Led4Static = Led4::new_static();
    let led4 = Led4::new(&LED4_STATIC, cell_pins, segment_pins, spawner)?;

    let offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
    let mut clock_time = ClockTime::new(offset_minutes);

    // Start the auto Wi-Fi, using the clock display for status.
    // cmk0 do we even need src/wifi.rs to be public? rename WifiAuto?
    let led4_ref = &led4;
    let (stack, mut button) = wifi_auto
        .connect(spawner, move |event| async move {
            use serials::wifi_auto::WifiAutoEvent;
            match event {
                WifiAutoEvent::CaptivePortalReady => {
                    led4_ref.write_text(BlinkState::BlinkingAndOn, ['C', 'O', 'N', 'N']);
                }
                WifiAutoEvent::Connecting { .. } => {
                    led4_ref.animate_text(circular_outline_animation(true));
                }
                WifiAutoEvent::Connected => {
                    led4_ref.write_text(BlinkState::Solid, ['D', 'O', 'N', 'E']);
                }
            }
        })
        .await?;

    // Every hour, check the time and fire an event.
    static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    let time_sync = TimeSync::new(&TIME_SYNC_STATIC, stack, spawner);

    // WiFi is connected, start in normal clock mode
    let mut state = State::HoursMinutes;

    // Main display loop
    loop {
        state = state
            .execute(&mut clock_time, &mut button, &time_sync, &led4)
            .await;

        // Save timezone offset to flash when it changes
        let current_offset_minutes = clock_time.offset_minutes();
        if current_offset_minutes != offset_minutes {
            let _ = timezone_field.set_offset_minutes(current_offset_minutes);
        }
    }
}

// State machine for 4-digit LED clock display modes and transitions.

/// Display states for the 4-digit LED clock.
#[derive(Debug, defmt::Format, Clone, Copy, Default)]
pub enum State {
    #[default]
    HoursMinutes,
    MinutesSeconds,
    EditOffset,
}

impl State {
    /// Execute the state machine for this clock state.
    async fn execute(
        self,
        clock_time: &mut ClockTime,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        led4: &Led4<'_>,
    ) -> Self {
        match self {
            Self::HoursMinutes => {
                self.execute_hours_minutes(clock_time, button, time_sync, led4)
                    .await
            }
            Self::MinutesSeconds => {
                self.execute_minutes_seconds(clock_time, button, time_sync, led4)
                    .await
            }
            Self::EditOffset => self.execute_edit_offset(clock_time, button, led4).await,
        }
    }

    async fn execute_hours_minutes(
        self,
        clock_time: &mut ClockTime,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        led4: &Led4<'_>,
    ) -> Self {
        let (hours, minutes, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        led4.write_text(
            BlinkState::Solid,
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
        );

        match select(
            select(button.press_duration(), Timer::after(sleep_duration)),
            time_sync.wait(),
        )
        .await
        {
            Either::First(Either::First(PressDuration::Short)) => Self::MinutesSeconds,
            Either::First(Either::First(PressDuration::Long)) => Self::EditOffset,
            Either::First(Either::Second(_)) => self, // Timer elapsed
            Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                info!(
                    "Time sync success: setting clock to {}",
                    unix_seconds.as_i64()
                );
                clock_time.set_from_unix(unix_seconds);
                self
            }
            Either::Second(TimeSyncEvent::Failed(msg)) => {
                info!("Time sync failed: {}", msg);
                self
            }
        }
    }

    async fn execute_minutes_seconds(
        self,
        clock_time: &mut ClockTime,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        led4: &Led4<'_>,
    ) -> Self {
        let (_, minutes, seconds, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_SECOND);
        led4.write_text(
            BlinkState::Solid,
            [
                tens_digit(minutes),
                ones_digit(minutes),
                tens_digit(seconds),
                ones_digit(seconds),
            ],
        );

        match select(
            select(button.press_duration(), Timer::after(sleep_duration)),
            time_sync.wait(),
        )
        .await
        {
            Either::First(Either::First(PressDuration::Short)) => Self::HoursMinutes,
            Either::First(Either::First(PressDuration::Long)) => Self::EditOffset,
            Either::First(Either::Second(_)) => self, // Timer elapsed
            Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                info!(
                    "Time sync success: setting clock to {}",
                    unix_seconds.as_i64()
                );
                clock_time.set_from_unix(unix_seconds);
                self
            }
            Either::Second(TimeSyncEvent::Failed(msg)) => {
                info!("Time sync failed: {}", msg);
                self
            }
        }
    }

    async fn execute_edit_offset(
        self,
        clock_time: &mut ClockTime,
        button: &mut Button<'_>,
        led4: &Led4<'_>,
    ) -> Self {
        let (hours, minutes, _, _) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        led4.write_text(
            BlinkState::BlinkingAndOn,
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
        );

        match select(
            button.press_duration(),
            Timer::after(Duration::from_millis(500)),
        )
        .await
        {
            Either::First(PressDuration::Short) => {
                let new_offset_minutes = clock_time.offset_minutes() + 60;
                clock_time.set_offset_minutes(new_offset_minutes);
                Self::EditOffset
            }
            Either::First(PressDuration::Long) => Self::HoursMinutes,
            Either::Second(_) => self, // Timer elapsed
        }
    }
}

// cmk attach to an impl
#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Value < 60 ensures division is safe"
)]
const fn tens_digit(value: u8) -> char {
    ((value / 10) + b'0') as char
}

#[inline]
const fn tens_hours(value: u8) -> char {
    if value >= 10 { '1' } else { ' ' }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::integer_division_remainder_used,
    reason = "Value < 60 ensures division is safe"
)]
const fn ones_digit(value: u8) -> char {
    ((value % 10) + b'0') as char
}
