//! Dual servo control example.
//! Moves two servos back and forth for 2 seconds using both channels of one PWM slice.
//! Connect servos to GPIO 0 and GPIO 1.

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::pwm::{Config, Pwm};
use embassy_time::Timer;
use panic_probe as _;
use serials::servo_pair::ServoPair;

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("Starting dual servo example");

    // Create two servos on the same PWM slice using both channels A and B
    let pwm = Pwm::new_output_ab(p.PWM_SLICE0, p.PIN_0, p.PIN_1, Config::default());
    let mut servos = ServoPair::new(pwm, 500, 2500, 500, 2500);

    info!("Moving servos back and forth for 2 seconds");

    let start = embassy_time::Instant::now();
    let duration = embassy_time::Duration::from_secs(2);

    loop {
        let elapsed = start.elapsed();
        if elapsed > duration {
            break;
        }

        // Move servos in opposite directions
        info!("Position: servo A=0°, servo B=180°");
        servos.set_degrees_a(0);
        servos.set_degrees_b(180);
        Timer::after_millis(500).await;

        // Move servos in opposite directions (swapped)
        info!("Position: servo A=180°, servo B=0°");
        servos.set_degrees_a(180);
        servos.set_degrees_b(0);
        Timer::after_millis(500).await;
    }

    info!("Done! Centering servos");
    servos.center_a();
    servos.center_b();

    Timer::after_millis(500).await;
    
    info!("Relaxing servos");
    servos.disable();

    Timer::after_secs(5).await;

    loop {
        info!("Sleeping");
        Timer::after_secs(5).await;
    }
}
