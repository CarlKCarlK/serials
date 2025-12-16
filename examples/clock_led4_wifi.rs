//! Wi-Fi enabled 4-digit clock that provisions credentials through `WifiSetup`.
//!
//! This example demonstrates how to pair the shared captive-portal workflow with the
//! `ClockLed4` state machine. The `WifiSetup` helper owns Wi-Fi onboarding while the
//! clock display reflects progress and, once connected, continues handling user input.

#![no_std]
#![no_main]
#![cfg(feature = "wifi")]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::pin::pin;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::{self, Level};
use panic_probe as _;
use serials::button::{Button, PressDuration};
use serials::clock::{Clock, ClockStatic, ONE_MINUTE, ONE_SECOND, h12_m_s};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::led4::{BlinkState, Led4, Led4Static, OutputArray, circular_outline_animation};
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
    info!("Starting Wi-Fi 4-digit clock (WifiSetup)");
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

    // Set up the LED4 display.
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

    // Connect Wi-Fi, using the clock display for status.
    let led4_ref = &led4;
    let (stack, mut button) = wifi_setup
        .connect(spawner, move |event| async move {
            use serials::wifi_setup::WifiSetupEvent;
            match event {
                WifiSetupEvent::CaptivePortalReady => {
                    led4_ref.write_text(['C', 'O', 'N', 'N'], BlinkState::BlinkingAndOn);
                }
                WifiSetupEvent::Connecting { .. } => {
                    led4_ref.animate_text(circular_outline_animation(true));
                }
                WifiSetupEvent::Connected => {
                    led4_ref.write_text(['D', 'O', 'N', 'E'], BlinkState::Solid);
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
                    .execute_hours_minutes(speed, &clock, &mut button, &time_sync, &led4)
                    .await?
            }
            State::MinutesSeconds => {
                state
                    .execute_minutes_seconds(&clock, &mut button, &time_sync, &led4)
                    .await?
            }
            State::EditOffset => {
                state
                    .execute_edit_offset(&clock, &mut button, &timezone_field, &led4)
                    .await?
            }
        };
    }
}

// State machine for 4-digit LED clock display modes and transitions.

/// Display states for the 4-digit LED clock.
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
        led4: &Led4<'_>,
    ) -> Result<Self> {
        clock.set_speed(speed).await;
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        led4.write_text(
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
            BlinkState::Solid,
        );
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
                    led4.write_text(
                        [
                            tens_hours(hours),
                            ones_digit(hours),
                            tens_digit(minutes),
                            ones_digit(minutes),
                        ],
                        BlinkState::Solid,
                    );
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
        led4: &Led4<'_>,
    ) -> Result<Self> {
        clock.set_speed(1.0).await;
        let (_, minutes, seconds) = h12_m_s(&clock.now_local());
        led4.write_text(
            [
                tens_digit(minutes),
                ones_digit(minutes),
                tens_digit(seconds),
                ones_digit(seconds),
            ],
            BlinkState::Solid,
        );
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
                    led4.write_text(
                        [
                            tens_digit(minutes),
                            ones_digit(minutes),
                            tens_digit(seconds),
                            ones_digit(seconds),
                        ],
                        BlinkState::Solid,
                    );
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
        led4: &Led4<'_>,
    ) -> Result<Self> {
        info!("Entering edit offset mode");

        // Blink current hours and minutes
        let (hours, minutes, _) = h12_m_s(&clock.now_local());
        led4.write_text(
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
            BlinkState::BlinkingAndOn,
        );

        // Get the current offset minutes from clock (source of truth)
        let mut offset_minutes = clock.offset_minutes();
        info!("Current offset: {} minutes", offset_minutes);

        clock.set_tick_interval(None).await; // Disable ticks in edit mode
        clock.set_speed(1.0).await;
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
                    led4.write_text(
                        [
                            tens_hours(hours),
                            ones_digit(hours),
                            tens_digit(minutes),
                            ones_digit(minutes),
                        ],
                        BlinkState::BlinkingAndOn,
                    );
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
