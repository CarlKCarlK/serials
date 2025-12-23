#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::ir::{Ir, IrEvent, IrStatic};
use embassy_executor::Spawner;
use panic_probe as _;

// cmk make an inner-main and remove the unwrap...panic
#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("IR NEC decoder example starting...");

    static IR_STATIC: IrStatic = Ir::new_static();
    let ir = Ir::new(&IR_STATIC, p.PIN_15, spawner)
        .unwrap_or_else(|e| panic!("Failed to initialize IR receiver: {:?}", e));

    info!("IR receiver initialized on GP15");

    // Main loop: process IR events
    loop {
        let event = ir.wait_for_press().await;
        match event {
            IrEvent::Press { addr, cmd } => {
                info!("IR Button Press - addr=0x{:04X} cmd=0x{:02X}", addr, cmd);
            }
        }
    }
}
