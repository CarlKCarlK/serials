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
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Instant, Timer};
use panic_probe as _;
use serials::Result;
use serials::flash_array::{FlashArray, FlashArrayStatic};
use serials::button::{Button, PressDuration};
use serials::clock_time::{ClockTime, ONE_MINUTE, ONE_SECOND};
use serials::led4::{BlinkState, Led4, Led4Static, OutputArray};
use serials::time_sync::{TimeSync, TimeSyncEvent, TimeSyncStatic};
use serials::unix_seconds::UnixSeconds;
use serials::wifi_auto::fields::{TimezoneField, TimezoneFieldStatic};
use serials::wifi_auto::{WifiAuto, WifiAutoEvent, WifiAutoStatic};

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

    // cmk0 look at the clock docs
    static CLOCK_LED4_STATIC: ClockLed4Static = ClockLed4::new_static();
    let mut clock_led4 = ClockLed4::new(
        &CLOCK_LED4_STATIC,
        cell_pins,
        segment_pins,
        timezone_field,
        spawner,
    )?;

    // Start the auto Wi-Fi, using the clock display for status.
    let clock_led4_ref = &clock_led4;
    // cmk0 do we even need src/wifi.rs to be public? rename WifiAuto?
    let (stack, mut button) = wifi_auto
        .connect(spawner, move |event| {
            async move {
                match event {
                    WifiAutoEvent::CaptivePortalReady => {
                        clock_led4_ref
                            .set_state(ClockLed4State::CaptivePortalReady)
                            .await;
                    }
                    // cmk0 the Connecting does the animation itself. Shouldn't it just use led4's animation_text method?
                    // cmk0 can/should we move the circular animations into led4?
                    WifiAutoEvent::Connecting { .. } => {
                        clock_led4_ref.set_state(ClockLed4State::Connecting).await;
                    }
                    WifiAutoEvent::Connected => {
                        clock_led4_ref.set_state(ClockLed4State::HoursMinutes).await;
                    }
                }
            }
        })
        .await?;

    // When the wi-fi is connected, we get an internet stack and the button.

    // Every hour, check the time and fire an event.
    static TIME_SYNC_STATIC: TimeSyncStatic = TimeSync::new_static();
    let time_sync = TimeSync::new(&TIME_SYNC_STATIC, stack, spawner);

    // Run the clock. It will monitor button pushes and time sync events.
    clock_led4.check_button(&mut button, &time_sync).await
}

// State machine for 4-digit LED clock display modes and transitions.

/// Display states for the 4-digit LED clock.
#[derive(Debug, defmt::Format, Clone, Copy, Default)]
pub enum ClockLed4State {
    #[default]
    HoursMinutes,
    Connecting,
    MinutesSeconds,
    EditOffset,
    CaptivePortalReady,
}

impl ClockLed4State {
    /// Execute the state machine for this clock state.
    pub async fn execute(
        self,
        clock: &mut ClockLed4<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        match self {
            Self::HoursMinutes => self.execute_hours_minutes(clock, button, time_sync).await,
            Self::Connecting => self.execute_connecting(clock, time_sync).await,
            Self::MinutesSeconds => self.execute_minutes_seconds(clock, button, time_sync).await,
            Self::EditOffset => self.execute_edit_offset(clock, button).await,
            Self::CaptivePortalReady => self.execute_captive_portal_setup(clock, time_sync).await,
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

    async fn execute_connecting(self, clock: &ClockLed4<'_>, time_sync: &TimeSync) -> Self {
        clock.set_state(self).await;
        let deadline_ticks = Instant::now()
            .as_ticks()
            .saturating_add(ONE_MINUTE.as_ticks());

        loop {
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
                    success @ TimeSyncEvent::Success { .. } => {
                        Self::handle_time_sync_event(clock, success).await;
                        return Self::HoursMinutes;
                    }
                    failure @ TimeSyncEvent::Failed(_) => {
                        Self::handle_time_sync_event(clock, failure).await;
                    }
                },
                Err(_) => return Self::CaptivePortalReady,
            }
        }
    }

    async fn execute_hours_minutes(
        self,
        clock: &ClockLed4<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        clock.set_state(self).await;
        match select(button.press_duration(), time_sync.wait()).await {
            Either::First(PressDuration::Short) => Self::MinutesSeconds,
            Either::First(PressDuration::Long) => Self::EditOffset,
            Either::Second(event) => {
                Self::handle_time_sync_event(clock, event).await;
                self
            }
        }
    }

    async fn execute_minutes_seconds(
        self,
        clock: &ClockLed4<'_>,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        clock.set_state(self).await;
        match select(button.press_duration(), time_sync.wait()).await {
            Either::First(PressDuration::Short) => Self::HoursMinutes,
            Either::First(PressDuration::Long) => Self::EditOffset,
            Either::Second(event) => {
                Self::handle_time_sync_event(clock, event).await;
                self
            }
        }
    }

    async fn execute_edit_offset(self, clock: &ClockLed4<'_>, button: &mut Button<'_>) -> Self {
        clock.set_state(self).await;
        match button.press_duration().await {
            PressDuration::Short => {
                clock.adjust_offset_hours(1).await;
                clock.set_state(Self::EditOffset).await;
                Self::EditOffset
            }
            PressDuration::Long => Self::HoursMinutes,
        }
    }

    async fn execute_captive_portal_setup(
        self,
        clock: &ClockLed4<'_>,
        time_sync: &TimeSync,
    ) -> Self {
        clock.set_state(self).await;
        loop {
            match time_sync.wait().await {
                success @ TimeSyncEvent::Success { .. } => {
                    Self::handle_time_sync_event(clock, success).await;
                    return Self::HoursMinutes;
                }
                failure @ TimeSyncEvent::Failed(_) => {
                    Self::handle_time_sync_event(clock, failure).await;
                }
            }
        }
    }

    async fn handle_time_sync_event(clock: &ClockLed4<'_>, event: TimeSyncEvent) {
        match event {
            TimeSyncEvent::Success { unix_seconds } => {
                info!(
                    "Time sync success: setting clock to {}",
                    unix_seconds.as_i64()
                );
                clock.set_time_from_unix(unix_seconds).await;
            }
            TimeSyncEvent::Failed(msg) => {
                info!("Time sync failed: {}", msg);
            }
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

/// A device abstraction for 4-digit LED clocks.
pub struct ClockLed4<'a> {
    commands: &'a ClockLed4OuterStatic,
    #[allow(dead_code)] // Used for atomic sharing with device loop
    offset_mirror: &'a AtomicI32,
}

/// Static type for the `ClockLed4` device abstraction.
pub struct ClockLed4Static {
    commands: ClockLed4OuterStatic,
    led: Led4Static,
    offset_minutes: AtomicI32,
}

/// Channel type for sending commands to the `ClockLed4` device.
pub type ClockLed4OuterStatic = Channel<CriticalSectionRawMutex, ClockLed4Command, 4>;

impl ClockLed4Static {
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            commands: Channel::new(),
            led: Led4::new_static(),
            offset_minutes: AtomicI32::new(0),
        }
    }

    fn commands(&'static self) -> &'static ClockLed4OuterStatic {
        &self.commands
    }

    fn led(&'static self) -> &'static Led4Static {
        &self.led
    }

    fn offset_mirror(&'static self) -> &'static AtomicI32 {
        &self.offset_minutes
    }
}

impl ClockLed4<'_> {
    /// Create a new `ClockLed4` instance, which entails starting an Embassy task.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        clock_led4_static: &'static ClockLed4Static,
        cell_pins: OutputArray<'static, 4>,
        segment_pins: OutputArray<'static, 8>,
        #[cfg(all(feature = "wifi", not(feature = "host")))]
        timezone_field: &'static TimezoneField,
        spawner: Spawner,
    ) -> Result<Self> {
        let led4 = Led4::new(clock_led4_static.led(), cell_pins, segment_pins, spawner)?;
        #[cfg(all(feature = "wifi", not(feature = "host")))]
        let offset_minutes = timezone_field.offset_minutes()?.unwrap_or(0);
        #[cfg(not(all(feature = "wifi", not(feature = "host"))))]
        let offset_minutes = 0;
        let token = clock_led4_device_loop(
            clock_led4_static.commands(),
            led4,
            offset_minutes,
            clock_led4_static.offset_mirror(),
            #[cfg(all(feature = "wifi", not(feature = "host")))]
            timezone_field,
        )?;
        spawner.spawn(token);
        Ok(Self {
            commands: clock_led4_static.commands(),
            offset_mirror: clock_led4_static.offset_mirror(),
        })
    }

    /// Creates a new `ClockLed4Static` instance.
    #[must_use]
    pub const fn new_static() -> ClockLed4Static {
        ClockLed4Static::new_static()
    }

    /// Set the clock state directly.
    pub async fn set_state(&self, clock_state: ClockLed4State) {
        self.commands
            .send(ClockLed4Command::SetState(clock_state))
            .await;
    }

    /// Run the clock state machine loop.
    ///
    /// This method runs indefinitely, executing the state machine and handling
    /// button presses and time sync events. It should be called after WiFi
    /// connection is established and time sync is available.
    pub async fn check_button(
        &mut self,
        button: &mut Button<'_>,
        time_sync: &TimeSync,
    ) -> ! {
        let mut clock_state = ClockLed4State::HoursMinutes;
        loop {
            clock_state = clock_state.execute(self, button, time_sync).await;
        }
    }

    /// Set the time from Unix seconds.
    pub async fn set_time_from_unix(&self, unix_seconds: UnixSeconds) {
        self.commands
            .send(ClockLed4Command::SetTimeFromUnix(unix_seconds))
            .await;
    }

    /// Adjust the UTC offset by the given number of hours.
    pub async fn adjust_offset_hours(&self, hours: i32) {
        self.commands
            .send(ClockLed4Command::AdjustOffsetHours(hours))
            .await;
    }
}

/// Commands sent to the 4-digit LED clock device.
pub enum ClockLed4Command {
    SetState(ClockLed4State),
    SetTimeFromUnix(UnixSeconds),
    AdjustOffsetHours(i32),
}

impl ClockLed4Command {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The += operator wraps to always produce a result less than one day."
    )]
    fn apply(self, clock_time: &mut ClockTime, clock_state: &mut ClockLed4State) {
        match self {
            Self::SetTimeFromUnix(unix_seconds) => {
                clock_time.set_from_unix(unix_seconds);
            }
            Self::SetState(new_clock_mode) => {
                *clock_state = new_clock_mode;
            }
            Self::AdjustOffsetHours(hours) => {
                clock_time.adjust_offset_hours(hours);
            }
        }
    }
}

#[embassy_executor::task]
async fn clock_led4_device_loop(
    clock_commands: &'static ClockLed4OuterStatic,
    blinker: Led4<'static>,
    initial_offset_minutes: i32,
    offset_mirror: &'static AtomicI32,
    #[cfg(all(feature = "wifi", not(feature = "host")))]
    timezone_field: &'static TimezoneField,
) -> ! {
    let mut clock_time = ClockTime::new(initial_offset_minutes, offset_mirror);
    let mut clock_state = ClockLed4State::default();
    #[cfg(all(feature = "wifi", not(feature = "host")))]
    let mut persisted_offset_minutes = initial_offset_minutes;

    loop {
        let (blink_mode, text, sleep_duration) = clock_state.render(&clock_time);
        blinker.write_text(blink_mode, text);

        #[cfg(feature = "display-trace")]
        info!("Sleep for {:?}", sleep_duration);
        if let Either::First(notification) =
            select(clock_commands.receive(), Timer::after(sleep_duration)).await
        {
            notification.apply(&mut clock_time, &mut clock_state);
        }

        // Save timezone offset to flash when it changes.
        #[cfg(all(feature = "wifi", not(feature = "host")))]
        {
            let current_offset_minutes = offset_mirror.load(Ordering::Relaxed);
            if current_offset_minutes != persisted_offset_minutes {
                let _ = timezone_field.set_offset_minutes(current_offset_minutes);
                persisted_offset_minutes = current_offset_minutes;
            }
        }
    }
}
