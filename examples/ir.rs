#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::Pull;
use lib::{IrNec, IrNecEvent, IrNecNotifier};
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("IR NEC decoder example starting...");

    // Create the notifier channel
    static NOTIFIER: IrNecNotifier = IrNec::notifier();

    // Initialize the IR receiver on GP28 with pull-up (active-low IR modules idle HIGH)
    let ir = IrNec::new(p.PIN_28, Pull::Up, &NOTIFIER, spawner)
        .expect("Failed to initialize IR receiver");

    info!("IR receiver initialized on GP28");

    // Main loop: process IR events
    loop {
        let event = ir.wait().await;
        match event {
            IrNecEvent::Press { addr, cmd } => {
                info!("IR Button Press - addr=0x{:02X} cmd=0x{:02X}", addr, cmd);
            }
        }
    }
}
