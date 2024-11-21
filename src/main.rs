#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use embassy_executor::Spawner;
use pins::Pins;
use state_machine::{state_to_state, AdjustableClock, State};
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
    let mut adjustable_clock = AdjustableClock::default();
    loop {
        defmt::info!("State: {:?}", state);
        state = state_to_state(
            state,
            &mut virtual_display,
            &mut button,
            &mut adjustable_clock,
        )
        .await;
    }
}
