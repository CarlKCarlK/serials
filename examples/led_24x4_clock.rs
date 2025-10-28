#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::{Timer, Duration};
use lib::{define_led_strip, Led24x4};
use smart_leds::RGB8;
use panic_probe as _;

// 4x12 panel (48 pixels) using PIO1, SM0, DMA_CH1, GPIO16
// Max current 50 mA
define_led_strip! {
    led_strip2 as LedStrip2 {
        task: led_strip_2_driver,
        pio: PIO1,
        irq: PIO1_IRQ_0,
        sm: { field: sm0, index: 0 },
        dma: DMA_CH1,
        pin: PIN_16,
        len: 48,
        max_current_ma: 100
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    static LED_STRIP_NOTIFIER: LedStrip2::Notifier = LedStrip2::notifier();

    let led_strip = LedStrip2::new(
        spawner,
        &LED_STRIP_NOTIFIER,
        peripherals.PIO1,
        peripherals.DMA_CH1,
        peripherals.PIN_16,
    )
    .expect("Failed to start LED strip");

    // Wrap into virtual 4-char display
    let mut display = Led24x4::new(led_strip);

    info!("24x4 demo - simulated clock");

    // Simulated clock: loop from 12:00 -> 11:59 with ~1 second per minute
    // Cycle: 12 hours * 60 minutes = 720 minutes
    let mut minute = 0u16;
    let colors = [
        RGB8::new(255, 0, 0),     // digit 1: red
        RGB8::new(0, 255, 0),     // digit 2: green
        RGB8::new(0, 0, 255),     // digit 3: blue
        RGB8::new(255, 255, 0),   // digit 4: yellow
    ];

    loop {
        // Calculate hour in 12-hour format
        let hour = ((minute / 60) % 12) as u16;
        let hour_display = if hour == 0 { 12 } else { hour };
        let min = minute % 60;

        // Format as HH:MM (4 digits)
        let h1 = (hour_display / 10) as u8;
        let h2 = (hour_display % 10) as u8;
        let m1 = (min / 10) as u8;
        let m2 = (min % 10) as u8;

        let chars = [
            if h1 == 0 { ' ' } else { (h1 + b'0') as char },
            (h2 + b'0') as char,
            (m1 + b'0') as char,
            (m2 + b'0') as char,
        ];

        display.display(chars, colors).await.expect("display failed");
        info!("Clock: {:02}:{:02}", hour_display, min);

        // Sleep ~100ms (simulates 1 minute on clock - 10x faster)
        Timer::after(Duration::from_millis(100)).await;

        minute += 1;
        if minute >= 720 {
            minute = 0; // Loop after 12 hours
        }
    }
}
