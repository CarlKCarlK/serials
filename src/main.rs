#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant};
use pins::Pins;
use state_machine::{state_to_state, State};
use virtual_display::{Notifier, VirtualDisplay, CELL_COUNT0};
use {defmt_rtt as _, panic_probe as _};

mod bit_matrix;
mod leds;
mod pins;
mod state_machine;
mod virtual_display;

// cmk put in Brad's err catcher in place of unwrap!

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) {
    let (pins, _core1) = Pins::new_and_core1();

    static NOTIFIER0: Notifier<CELL_COUNT0> = VirtualDisplay::new_notifier();
    let mut virtual_display =
        VirtualDisplay::new(pins.cells0, pins.segments0, &NOTIFIER0, spawner0);

    let mut state = State::First;
    let mut button = pins.button;
    let start = Instant::now();
    let mut offset = Duration::default(); // cmk should offset me a virtual thing
    loop {
        defmt::info!("State: {:?}", state);
        (state, offset) =
            state_to_state(state, &mut virtual_display, &mut button, start, offset).await;
    }
}
