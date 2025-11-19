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

use core::sync::atomic::{AtomicI32, Ordering};
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::{self, Level};
use embassy_time::{Duration, Instant, Timer};
use panic_probe as _;
use serials::Result;
use serials::button::{Button, PressDuration};
use serials::clock_time::{ClockTime, ONE_MINUTE, ONE_SECOND};
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::led4::{BlinkState, Led4, Led4Static, OutputArray};
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

    // Define HTML etc for asking for timezone on the captive portal.
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

    // Define a clock with an LED4 display.
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
    static OFFSET_MIRROR: AtomicI32 = AtomicI32::new(0);
    let mut clock_time = ClockTime::new(offset_minutes, &OFFSET_MIRROR);
    let mut state = State::Connecting;

    // Start the auto Wi-Fi, using the clock display for status.
    // cmk0 do we even need src/wifi.rs to be public? rename WifiAuto?
    let (stack, mut button) = wifi_auto
        .connect(spawner, move |_event| async move {
            // WiFi events don't need to change state - the main loop handles it
        })
        .await?;

    // Every hour, check the time and fire an event.
    static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    let time_sync = TimeSync::new(&TIME_SYNC_STATIC, stack, spawner);

    // Main display loop
    loop {
        let (blink_mode, text, sleep_duration) = state.render(&clock_time);
        led4.write_text(blink_mode, text);

        state = state
            .execute(&mut clock_time, &mut button, &time_sync, sleep_duration)
            .await;

        // Save timezone offset to flash when it changes
        let current_offset_minutes = OFFSET_MIRROR.load(Ordering::Relaxed);
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
    Connecting,
    MinutesSeconds,
    EditOffset,
    CaptivePortalReady,
}

impl State {
    /// Execute the state machine for this clock state.
    async fn execute(
        self,
        clock_time: &mut ClockTime,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        sleep_duration: Duration,
    ) -> Self {
        match self {
            Self::HoursMinutes => {
                self.execute_hours_minutes(clock_time, button, time_sync, sleep_duration)
                    .await
            }
            Self::Connecting => self.execute_connecting(clock_time, time_sync).await,
            Self::MinutesSeconds => {
                self.execute_minutes_seconds(clock_time, button, time_sync, sleep_duration)
                    .await
            }
            Self::EditOffset => {
                self.execute_edit_offset(clock_time, button, sleep_duration)
                    .await
            }
            Self::CaptivePortalReady => {
                self.execute_captive_portal_setup(clock_time, time_sync, sleep_duration)
                    .await
            }
        }
    }

    /// Render the current clock state to display output.
    pub fn render(self, clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        match self {
            Self::HoursMinutes => Self::render_hours_minutes(clock_time),
            Self::Connecting => Self::render_connecting(clock_time),
            Self::MinutesSeconds => Self::render_minutes_seconds(clock_time),
            Self::EditOffset => Self::render_edit_offset(clock_time),
            Self::CaptivePortalReady => Self::render_captive_portal_setup(),
        }
    }

    async fn execute_connecting(self, clock_time: &mut ClockTime, time_sync: &TimeSync) -> Self {
        let deadline_ticks = Instant::now()
            .as_ticks()
            .saturating_add(ONE_MINUTE.as_ticks());

        let now_ticks = Instant::now().as_ticks();
        if now_ticks >= deadline_ticks {
            return Self::CaptivePortalReady;
        }

        let remaining_ticks = deadline_ticks - now_ticks;
        if remaining_ticks == 0 {
            return Self::CaptivePortalReady;
        }

        let timeout = Duration::from_ticks(remaining_ticks);
        match embassy_time::with_timeout(timeout, time_sync.wait()).await {
            Ok(event) => match event {
                TimeSyncEvent::Success { unix_seconds } => {
                    info!(
                        "Time sync success: setting clock to {}",
                        unix_seconds.as_i64()
                    );
                    clock_time.set_from_unix(unix_seconds);
                    Self::HoursMinutes
                }
                TimeSyncEvent::Failed(msg) => {
                    info!("Time sync failed: {}", msg);
                    self
                }
            },
            Err(_) => Self::CaptivePortalReady,
        }
    }

    async fn execute_hours_minutes(
        self,
        clock_time: &mut ClockTime,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
        sleep_duration: Duration,
    ) -> Self {
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
        sleep_duration: Duration,
    ) -> Self {
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
        sleep_duration: Duration,
    ) -> Self {
        match select(button.press_duration(), Timer::after(sleep_duration)).await {
            Either::First(PressDuration::Short) => {
                clock_time.adjust_offset_hours(1);
                Self::EditOffset
            }
            Either::First(PressDuration::Long) => Self::HoursMinutes,
            Either::Second(_) => self, // Timer elapsed
        }
    }

    async fn execute_captive_portal_setup(
        self,
        clock_time: &mut ClockTime,
        time_sync: &TimeSync,
        sleep_duration: Duration,
    ) -> Self {
        match select(time_sync.wait(), Timer::after(sleep_duration)).await {
            Either::First(TimeSyncEvent::Success { unix_seconds }) => {
                info!(
                    "Time sync success: setting clock to {}",
                    unix_seconds.as_i64()
                );
                clock_time.set_from_unix(unix_seconds);
                Self::HoursMinutes
            }
            Either::First(TimeSyncEvent::Failed(msg)) => {
                info!("Time sync failed: {}", msg);
                self
            }
            Either::Second(_) => self, // Timer elapsed
        }
    }

    fn render_hours_minutes(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        let (hours, minutes, _, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            BlinkState::Solid,
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
            sleep_duration,
        )
    }

    fn render_connecting(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        const FRAME_DURATION: Duration = Duration::from_millis(120);
        const TOP: char = '\'';
        const TOP_RIGHT: char = '"';
        const RIGHT: char = '>';
        const BOTTOM_RIGHT: char = ')';
        const BOTTOM: char = '_';
        const BOTTOM_LEFT: char = '*';
        const LEFT: char = '<';
        const TOP_LEFT: char = '(';
        const FRAMES: [[char; 4]; 8] = [
            [TOP, TOP, TOP, TOP],
            [TOP, TOP, TOP, TOP_RIGHT],
            [' ', ' ', ' ', RIGHT],
            [' ', ' ', ' ', BOTTOM_RIGHT],
            [BOTTOM, BOTTOM, BOTTOM, BOTTOM],
            [BOTTOM_LEFT, BOTTOM, BOTTOM, BOTTOM],
            [LEFT, ' ', ' ', ' '],
            [TOP_LEFT, TOP, TOP, TOP],
        ];

        let frame_duration_ticks = FRAME_DURATION.as_ticks();
        let frame_index = if frame_duration_ticks == 0 {
            0
        } else {
            let now_ticks = clock_time.now().as_ticks();
            ((now_ticks / frame_duration_ticks) % FRAMES.len() as u64) as usize
        };

        (BlinkState::Solid, FRAMES[frame_index], FRAME_DURATION)
    }

    fn render_minutes_seconds(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        let (_, minutes, seconds, sleep_duration) = clock_time.h_m_s_sleep_duration(ONE_SECOND);
        (
            BlinkState::Solid,
            [
                tens_digit(minutes),
                ones_digit(minutes),
                tens_digit(seconds),
                ones_digit(seconds),
            ],
            sleep_duration,
        )
    }

    fn render_edit_offset(clock_time: &ClockTime) -> (BlinkState, [char; 4], Duration) {
        let (hours, minutes, _, _) = clock_time.h_m_s_sleep_duration(ONE_MINUTE);
        (
            BlinkState::BlinkingAndOn,
            [
                tens_hours(hours),
                ones_digit(hours),
                tens_digit(minutes),
                ones_digit(minutes),
            ],
            Duration::from_millis(500),
        )
    }

    fn render_captive_portal_setup() -> (BlinkState, [char; 4], Duration) {
        (
            BlinkState::BlinkingAndOn,
            ['C', 'O', 'n', 'n'],
            Duration::from_millis(500),
        )
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
