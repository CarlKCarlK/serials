//! Dual servo control example.
//! Moves two servos back and forth for 2 seconds.
//! Connect servos to GPIO 0 and GPIO 1 (they will use different PWM slices).

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use serials::servo_a;

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("Starting dual servo example");

    // Create two servos on different PWM slices
    // GPIO 0 is on PWM slice 0, GPIO 1 is on PWM slice 0 (but we can't share)
    // So use GPIO 0 (slice 0) and GPIO 2 (slice 1)
    let mut servo1 = servo_a!(p.PWM_SLICE0, p.PIN_0, 500, 2500);
    let mut servo2 = servo_a!(p.PWM_SLICE1, p.PIN_2, 500, 2500);

    info!("Moving servos back and forth for 2 seconds");

    let start = embassy_time::Instant::now();
    let duration = embassy_time::Duration::from_secs(2);

    loop {
        let elapsed = start.elapsed();
        if elapsed > duration {
            break;
        }

        // Move both servos to 0 degrees
        info!("Position: 0 degrees");
        servo1.set_degrees(0);
        servo2.set_degrees(0);
        Timer::after_millis(500).await;

        // Move both servos to 180 degrees
        info!("Position: 180 degrees");
        servo1.set_degrees(180);
        servo2.set_degrees(180);
        Timer::after_millis(500).await;
    }

    info!("Done! Centering servos");
    servo1.center();
    servo2.center();

    Timer::after_millis(500).await;
    
    info!("Relaxing servos");
    servo1.disable();
    servo2.disable();

    Timer::after_secs(5).await;

    loop {
        info!("Sleeping");
        Timer::after_secs(5).await;
    }
}
