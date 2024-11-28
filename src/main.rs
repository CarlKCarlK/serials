//! A 4-digit 7-segment clock that can be controlled by a button.
//!
//! Runs on a Raspberry Pi Pico RP2040. See the `README.md` for more information.
#![no_std]
#![no_main]
#![warn(
    clippy::pedantic,
    clippy::nursery,
    clippy::use_self,
    unused_lifetimes,
    missing_docs,
    single_use_lifetimes,
    unreachable_pub,
    // TODO: clippy::cargo,
    clippy::perf,
    clippy::style,
    clippy::complexity,
    clippy::correctness,
    clippy::must_use_candidate,
    // TODO: clippy::cargo_common_metadata
    clippy::unwrap_used, clippy::unwrap_used, // : Warns if you're using .unwrap() or .expect(), which can be a sign of inadequate error handling.
    clippy::panic_in_result_fn, // Ensures functions that return Result do not contain panic!, which could be inappropriate in production code.

)]
#![allow(clippy::future_not_send)] // This is a single-threaded application, so futures don't need to be Send.
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use lib::{Button, Clock, ClockNotifier, ClockState, Never, Result}; // This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Never> {
    let hardware = lib::Hardware::default();
    #[allow(clippy::items_after_statements)]
    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let mut clock = Clock::new(hardware.cells, hardware.segments, &CLOCK_NOTIFIER, spawner)?;
    let mut button = Button::new(hardware.button);
    info!("Clock and button created");

    // Run the state machine
    let mut state = ClockState::default();
    loop {
        defmt::info!("State: {:?}", state);
        state = state.run_and_next(&mut clock, &mut button).await;
    }
}

// TODO: Is testing possible?
