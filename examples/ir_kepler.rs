//! Example showing how to use the SunFounder Kepler Kit IR remote.
#![no_std]
#![no_main]
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use embassy_executor::Spawner;
use panic_probe as _;
use device_kit::ir_kepler::{IrKepler, IrKeplerStatic};

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    info!("Starting Kepler IR Remote Example");

    static IR_KEPLER_STATIC: IrKeplerStatic = IrKepler::new_static();
    let ir_kepler = IrKepler::new(&IR_KEPLER_STATIC, p.PIN_15, spawner)?;

    info!("Kepler remote initialized on GPIO 15");
    info!("Press buttons on the remote control...");

    loop {
        let button = ir_kepler.wait_for_press().await;
        info!("Button pressed: {:?}", button);
    }
}
