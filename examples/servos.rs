//! Dual servo control example.
//! Moves two servos in opposite directions for 2 seconds.
//! Connect servos to GPIO 0 and GPIO 2.

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use serials::servo::servo_even;

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("Starting dual servo example");

    // Create servos on GPIO 0 and GPIO 2 (both even pins)
    // GPIO 0 → (0/2) % 8 = 0 → PWM_SLICE0
    // GPIO 2 → (2/2) % 8 = 1 → PWM_SLICE1
    let mut servo0 = servo_even!(p.PIN_0, p.PWM_SLICE0, 500, 2500);
    let mut servo2 = servo_even!(p.PIN_2, p.PWM_SLICE1, 500, 2500);

    info!("Moving servos in opposite directions for 2 seconds");

    let start = embassy_time::Instant::now();
    let duration = embassy_time::Duration::from_secs(2);

    loop {
        let elapsed = start.elapsed();
        if elapsed > duration {
            break;
        }

        // Move servos in opposite directions
        info!("Position: servo0=0°, servo2=180°");
        servo0.set_degrees(0);
        servo2.set_degrees(180);
        Timer::after_millis(500).await;

        // Move servos in opposite directions (swapped)
        info!("Position: servo0=180°, servo2=0°");
        servo0.set_degrees(180);
        servo2.set_degrees(0);
        Timer::after_millis(500).await;
    }

    info!("Done! Centering servos");
    servo0.center();
    servo2.center();

    Timer::after_millis(500).await;
    
    info!("Relaxing servos");
    servo0.disable();
    servo2.disable();

    Timer::after_secs(5).await;

    loop {
        info!("Sleeping");
        Timer::after_secs(5).await;
    }
}
