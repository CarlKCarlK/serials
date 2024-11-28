//! # Clock cmk
#![no_std]
#![no_main]
#![warn(
    clippy::pedantic,
    // clippy::nursery, // leave off because I can't turn off it's send_sync warning.
    clippy::use_self,
    unused_lifetimes,
    missing_docs,
    single_use_lifetimes,
    unreachable_pub,
    // cmk clippy::cargo,
    clippy::perf,
    clippy::style,
    clippy::complexity,
    clippy::correctness,
    clippy::must_use_candidate,
    // // cmk0 clippy::cargo_common_metadata
    clippy::unwrap_used, clippy::unwrap_used, // : Warns if you're using .unwrap() or .expect(), which can be a sign of inadequate error handling.
    clippy::panic_in_result_fn, // Ensures functions that return Result do not contain panic!, which could be inappropriate in production code.

)]
use defmt::info;
use embassy_executor::Spawner;
// Importing from our own internal `lib` module
use defmt_rtt as _;
use lib::{Button, Clock, ClockNotifier, ClockState, Never, Result};
use panic_probe as _;
#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) -> ! {
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
        state = state.next_state(&mut clock, &mut button).await;
    }
}

// cmk what can we test?
