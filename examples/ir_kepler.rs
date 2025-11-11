//! Example showing how to use the SunFounder Kepler Kit IR remote.
#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use serials::ir_kepler::IrKepler;
use panic_probe as _;

static NOTIFIER: serials::ir::IrNotifier = IrKepler::notifier();

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("Starting Kepler IR Remote Example");

    let remote = IrKepler::new(p.PIN_15, &NOTIFIER, spawner)
        .unwrap_or_else(|_| core::panic!("Failed to initialize Kepler remote"));

    info!("Kepler remote initialized on GPIO 15");
    info!("Press buttons on the remote control...");

    loop {
        let button = remote.wait().await;
        info!("Button pressed: {:?}", button);
    }
}
