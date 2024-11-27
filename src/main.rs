#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// cmk what other's to turn on? (record in notes)
use defmt::info;
use embassy_executor::Spawner;
use lib::{Button, Clock, ClockNotifier, Never, Pins, Result, State};

use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

#[allow(clippy::items_after_statements)]
async fn inner_main(spawner0: Spawner) -> Result<Never> {
    let (pins, _core1) = Pins::new_and_core1(); // cmk good or bad?

    static NOTIFIER0: ClockNotifier = Clock::notifier();
    let mut clock = Clock::new(pins.cells0, pins.segments0, &NOTIFIER0, spawner0)?;
    info!("Clock created");

    let mut button = Button::new(pins.button);
    let mut state = State::default();
    loop {
        defmt::info!("State: {:?}", state);
        state = state.next_state(&mut clock, &mut button).await;
    }
}
