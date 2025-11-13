//! Example showing how to use the SunFounder Kepler Kit IR remote.
#![no_std]
#![no_main]
#![feature(never_type)]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use panic_probe as _;
use serials::ir_kepler::{IrKepler, IrKeplerNotifier};

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> serials::Result<!> {
    let p = embassy_rp::init(Default::default());

    info!("Starting Kepler IR Remote Example");

    static IR_KEPLER_NOTIFIER: IrKeplerNotifier = IrKepler::notifier();
    let ir_kepler = IrKepler::new(p.PIN_15, &IR_KEPLER_NOTIFIER, spawner)?;

    info!("Kepler remote initialized on GPIO 15");
    info!("Press buttons on the remote control...");

    loop {
        let button = ir_kepler.wait().await;
        info!("Button pressed: {:?}", button);
    }
}
