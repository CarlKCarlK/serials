#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use serials::led_strip::define_led_strips;
use smart_leds::RGB8;

// WS2812B 4x12 LED matrix (48 pixels)
// Uses PIO1, State Machine 0, DMA_CH1, GPIO16 (pin 21)
// Max 500mA current budget (safe for USB 2.0)
define_led_strips! {
    pio: PIO1,
    strips: [
        led_strip2 {
            sm: 0,
            dma: DMA_CH1,
            pin: PIN_16,
            len: 48,
            max_current_ma: 100
        }
    ]
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    // Initialize PIO1 bus
    let (pio_bus, sm0, _sm1, _sm2, _sm3) = pio1_split(peripherals.PIO1);

    static LED_STRIP_NOTIFIER: led_strip2::Notifier = led_strip2::notifier();
    let mut led_strip = led_strip2::new(
        spawner,
        &LED_STRIP_NOTIFIER,
        pio_bus,
        sm0,
        peripherals.DMA_CH1.into(),
        peripherals.PIN_16.into(),
    )
    .expect("Failed to start LED strip");

    info!("WS2812B 4x12 Matrix demo starting");
    info!("Using PIO1, DMA_CH1, GPIO16");
    info!(
        "Max brightness: {}  ({}mA budget)",
        led_strip2::MAX_BRIGHTNESS,
        500
    );
    info!("Wiring: Red->VSYS(pin39), White->GND(pin38), Green->GPIO16(pin21)");

    const SNAKE_LENGTH: usize = 5;
    const SNAKE_COLOR: RGB8 = RGB8::new(255, 255, 255); // Full white - will be scaled by max_brightness
    const BACKGROUND: RGB8 = RGB8::new(0, 0, 0);

    // Snake state: array buffer starts all background
    let mut frame = [BACKGROUND; led_strip2::LEN];

    let mut position: usize = 0;

    loop {
        // Turn on the head
        let head_pos = position % led_strip2::LEN;
        frame[head_pos] = SNAKE_COLOR;

        // Turn off the tail
        let tail_pos = (position + led_strip2::LEN - SNAKE_LENGTH) % led_strip2::LEN;
        frame[tail_pos] = BACKGROUND;

        // Send entire frame
        led_strip
            .update_pixels(&frame)
            .await
            .expect("pattern update failed");

        position = (position + 1) % led_strip2::LEN;
        Timer::after_millis(100).await;
    }
}
