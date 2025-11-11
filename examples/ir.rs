#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use serials::ir::{Ir, IrEvent, IrNotifier};
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("IR NEC decoder example starting...");

    // Create the notifier channel
    static NOTIFIER: IrNotifier = Ir::notifier();

    // Initialize the IR receiver on GP15 (uses Pull::Up for typical IR modules)
    let ir = Ir::new(p.PIN_15, &NOTIFIER, spawner)
        .unwrap_or_else(|e| panic!("Failed to initialize IR receiver: {:?}", e));

    info!("IR receiver initialized on GP15");

    // Main loop: process IR events
    loop {
        let event = ir.wait().await;
        match event {
            IrEvent::Press { addr, cmd } => {
                info!("IR Button Press - addr=0x{:04X} cmd=0x{:02X}", addr, cmd);
            }
        }
    }
}
