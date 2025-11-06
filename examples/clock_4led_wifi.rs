//! A 4-digit 7-segment clock with button control.
//!
//! This variant intentionally omits Wi-Fi and time synchronisation so it can be
//! used when the firmware is built without the `wifi` Cargo feature. The clock
//! starts at `12:00`, advances locally, and a short button press toggles the
//! display between `HH:MM` and `MM:SS`.

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{self, Level};
use lib::{
    Button, Clock4Led, Clock4LedNotifier, Clock4LedState, Led4Seg, Led4SegNotifier, OutputArray,
    PressDuration, Result,
};
use panic_probe as _;

// ============================================================================
// Main
// ============================================================================

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting 4-digit LED clock (no Wi-Fi)");

    // Initialize RP2040 peripherals
    let p = embassy_rp::init(Default::default());

    // Setup LED display pins
    let cell_pins = OutputArray::new([
        gpio::Output::new(p.PIN_1, Level::High),
        gpio::Output::new(p.PIN_2, Level::High),
        gpio::Output::new(p.PIN_3, Level::High),
        gpio::Output::new(p.PIN_4, Level::High),
    ]);

    let segment_pins = OutputArray::new([
        gpio::Output::new(p.PIN_5, Level::Low),
        gpio::Output::new(p.PIN_6, Level::Low),
        gpio::Output::new(p.PIN_7, Level::Low),
        gpio::Output::new(p.PIN_8, Level::Low),
        gpio::Output::new(p.PIN_9, Level::Low),
        gpio::Output::new(p.PIN_10, Level::Low),
        gpio::Output::new(p.PIN_11, Level::Low),
        gpio::Output::new(p.PIN_12, Level::Low),
    ]);

    // Create LED display device
    static LED_4SEG_NOTIFIER: Led4SegNotifier = Led4Seg::notifier();
    let led_display = Led4Seg::new(cell_pins, segment_pins, &LED_4SEG_NOTIFIER, spawner)?;

    // Store led_display in a static to pass to Clock4Led
    static LED_DISPLAY_CELL: static_cell::StaticCell<Led4Seg<'_>> = static_cell::StaticCell::new();
    let led_display_static = LED_DISPLAY_CELL.init(led_display);

    // Create Clock device
    static CLOCK_NOTIFIER: Clock4LedNotifier = Clock4Led::notifier();
    let clock = Clock4Led::new(led_display_static, &CLOCK_NOTIFIER, spawner)?;

    // Create Button
    let mut button = Button::new(gpio::Input::new(p.PIN_13, gpio::Pull::Down));

    info!("Clock and button created");

    // Track the current display mode and toggle it on short presses.
    let mut state = Clock4LedState::default();
    clock.set_state(state).await;

    loop {
        match button.press_duration().await {
            PressDuration::Short => {
                state = match state {
                    Clock4LedState::HoursMinutes => Clock4LedState::MinutesSeconds,
                    _ => Clock4LedState::HoursMinutes,
                };
                info!("Switching display mode: {:?}", state);
                clock.set_state(state).await;
            }
            PressDuration::Long => {
                // Ignore long presses for this minimal example.
                info!("Long press ignored");
            }
        }
    }
}
