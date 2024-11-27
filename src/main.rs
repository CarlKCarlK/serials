#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use button::Button;
use clock::{Clock, ClockNotifier};
use defmt::info;
use embassy_executor::Spawner;
use error::Result;
use never::Never;
use pins::Pins;
use state_machine::State;
use {defmt_rtt as _, panic_probe as _};

// cmk why doesn't brad use 'mod'?
mod bit_matrix;
mod blinker;
mod button;
mod clock;
mod display;
mod error;
mod leds;
mod never;
mod offset_time;
mod pins;
mod state_machine;

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

#[allow(clippy::items_after_statements)]
async fn inner_main(spawner0: Spawner) -> Result<Never> {
    info!("build time: {}", env!("BUILD_TIME"));
    let (pins, _core1) = Pins::new_and_core1(); // cmk good or bad?

    // cmk what would it look like to have another virtual display? Do we need CellCount0 here? should define a macro?
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
