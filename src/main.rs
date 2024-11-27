#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// cmk what other clippy's to turn on? (record in notes)
use defmt::info;
use embassy_executor::Spawner;
// Importing from our own internal `lib` module
use defmt_rtt as _;
use lib::{Button, Clock, ClockNotifier, Never, Result, State};
use panic_probe as _;

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

#[allow(clippy::items_after_statements)]
async fn inner_main(spawner: Spawner) -> Result<Never> {
    let hardware = lib::Hardware::default();

    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let mut clock = Clock::new(hardware.cells, hardware.segments, &CLOCK_NOTIFIER, spawner)?;
    let mut button = Button::new(hardware.button);
    info!("Clock and button created");

    // Run the state machine
    let mut state = State::default();
    loop {
        defmt::info!("State: {:?}", state);
        state = state.next_state(&mut clock, &mut button).await;
    }
}
