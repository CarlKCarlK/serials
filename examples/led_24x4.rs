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
        max_current_ma: 50
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    static LED_STRIP_NOTIFIER: LedStrip2::Notifier = LedStrip2::notifier();

    let mut led_strip = LedStrip2::new(
        spawner,
        &LED_STRIP_NOTIFIER,
        peripherals.PIO1,
        peripherals.DMA_CH1,
        peripherals.PIN_16,
    )
    .expect("Failed to start LED strip");

    // Wrap into virtual 4-char display
    let mut display = Led24x4::new(led_strip);

    info!("24x4 demo - displaying 1234");

    // Colors for each digit position
    let colors = [
        RGB8::new(255, 0, 0),   // 1: red
        RGB8::new(0, 255, 0),   // 2: green
        RGB8::new(0, 0, 255),   // 3: blue
        RGB8::new(255, 255, 0), // 4: yellow
    ];

    let chars = ['1', '2', '3', '4'];
    display.display(chars, colors).await.expect("display failed");
    info!("1234 displayed");

    // Hold forever
    loop {
        Timer::after(Duration::from_secs(10)).await;
    }
}
