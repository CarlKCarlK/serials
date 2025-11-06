//! Port of the `clock-wifi` example into the `serials` crate.
//! The Wi-Fi/time synchronisation subsystem is stubbed out so the firmware runs offline.

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use lib::cwf::{Clock, ClockNotifier, ClockState, Hardware, TimeSync, TimeSyncNotifier};
use lib::{Button, Result};
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let hardware = Hardware::default();

    static TIME_SYNC: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(&TIME_SYNC, spawner);

    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let mut clock = Clock::new(hardware.cells, hardware.segments, &CLOCK_NOTIFIER, spawner)?;
    let mut button = Button::new(hardware.button);
    info!("Clock and button created");

    let mut state = ClockState::default();
    loop {
        info!("State: {:?}", state);
        state = state.execute(&mut clock, &mut button, time_sync).await;
    }
}
