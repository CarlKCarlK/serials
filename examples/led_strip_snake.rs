#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use lib::define_led_strips;
use panic_probe as _;
use smart_leds::RGB8;

// WS2812B 4x12 LED matrix (48 pixels)
// Uses PIO1, State Machine 0, DMA_CH1, GPIO16 (pin 21)
// Max 500mA current budget (safe for USB 2.0)
define_led_strips! {
    led_strip2 as LedStrip2 {
        task: led_strip_2_driver,
        pio: PIO1,
        irq: PIO1_IRQ_0,
        irq_name: LedStrip2Irqs,
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
    let mut led_strip = LedStrip2::new(
        spawner,
        &LED_STRIP_NOTIFIER,
        peripherals.PIO1,
        peripherals.DMA_CH1,
        peripherals.PIN_16,
    )
    .expect("Failed to start LED strip");

    info!("WS2812B 4x12 Matrix demo starting");
    info!("Using PIO1, DMA_CH1, GPIO16");
    info!(
        "Max brightness: {}  ({}mA budget)",
        LedStrip2::MAX_BRIGHTNESS,
        500
    );
    info!("Wiring: Red->VSYS(pin39), White->GND(pin38), Green->GPIO16(pin21)");

    const SNAKE_LENGTH: usize = 5;
    const SNAKE_COLOR: RGB8 = RGB8::new(255, 255, 255); // Full white - will be scaled by max_brightness
    const BACKGROUND: RGB8 = RGB8::new(0, 0, 0);

    // Snake state: array buffer starts all background
    let mut frame = [BACKGROUND; LedStrip2::LEN];

    let mut position: usize = 0;

    loop {
        // Turn on the head
        let head_pos = position % LedStrip2::LEN;
        frame[head_pos] = SNAKE_COLOR;

        // Turn off the tail
        let tail_pos = (position + LedStrip2::LEN - SNAKE_LENGTH) % LedStrip2::LEN;
        frame[tail_pos] = BACKGROUND;

        // Send entire frame
        led_strip
            .update_pixels(&frame)
            .await
            .expect("pattern update failed");

        position = (position + 1) % LedStrip2::LEN;
        Timer::after_millis(100).await;
    }
}
