#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use button::Button;
use embassy_executor::Spawner;
use pins::Pins;
use state_machine::{AdjustableClock, State};
use virtual_display::{Notifier, VirtualDisplay, CELL_COUNT0};
use {defmt_rtt as _, panic_probe as _};

mod bit_matrix;
mod button;
mod leds;
mod pins;
mod state_machine;
mod virtual_display;

// cmk put in Brad's err catcher in place of unwrap!

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) {
    let (pins, _core1) = Pins::new_and_core1();

    // cmk what would it look like to have another virtual display? Do we need CellCount0 here? should define a macro?
    static NOTIFIER0: Notifier<CELL_COUNT0> = VirtualDisplay::new_notifier();
    let mut virtual_display =
        VirtualDisplay::new(pins.cells0, pins.segments0, &NOTIFIER0, spawner0);

    let mut button = Button::new(pins.button);
    let mut adjustable_clock = AdjustableClock::default();

    let mut state = State::default();
    loop {
        defmt::info!("State: {:?}", state);
        state = state
            .next_state(&mut virtual_display, &mut button, &mut adjustable_clock)
            .await;
    }
}
