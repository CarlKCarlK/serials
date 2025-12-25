#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::led_strip::Milliamps;
use device_kit::led_strip::define_led_strips_shared;
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use smart_leds::RGB8;

// WS2812B 4x12 LED matrix (48 pixels)
// Uses PIO1, State Machine 0, DMA_CH1, GPIO16 (pin 21)
// Max 500mA current budget (safe for USB 2.0)
define_led_strips_shared! {
    pio: PIO1,
    strips: [
        Gpio16LedStrip {
            sm: 0,
            dma: DMA_CH1,
            pin: PIN_16,
            len: 48,
            max_current: Milliamps(100)
        }
    ]
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    match inner_main(spawner).await {
        Ok(_) => unreachable!(),
        Err(e) => panic!("Fatal error: {:?}", e),
    }
}

async fn inner_main(spawner: Spawner) -> device_kit::Result<()> {
    let p = embassy_rp::init(Default::default());

    // Initialize PIO1 bus
    let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO1);

    let gpio16_led_strip = Gpio16LedStrip::new(sm0, p.DMA_CH1, p.PIN_16, spawner)?;

    info!("WS2812B 4x12 Matrix demo starting");
    info!("Using PIO1, DMA_CH1, GPIO16");
    info!(
        "Max brightness: {}  ({}mA budget)",
        Gpio16LedStrip::MAX_BRIGHTNESS,
        500
    );
    info!("Wiring: Red->VSYS(pin39), White->GND(pin38), Green->GPIO16(pin21)");

    const SNAKE_LENGTH: usize = 5;
    const SNAKE_COLOR: RGB8 = RGB8::new(255, 255, 255); // Full white - will be scaled by max_brightness
    const BACKGROUND: RGB8 = RGB8::new(0, 0, 0);

    // Snake state: array buffer starts all background
    let mut frame = [BACKGROUND; Gpio16LedStrip::LEN];

    let mut position: usize = 0;

    loop {
        // Turn on the head
        let head_pos = position % Gpio16LedStrip::LEN;
        frame[head_pos] = SNAKE_COLOR;

        // Turn off the tail
        let tail_pos = (position + Gpio16LedStrip::LEN - SNAKE_LENGTH) % Gpio16LedStrip::LEN;
        frame[tail_pos] = BACKGROUND;

        // Send entire frame
        gpio16_led_strip.update_pixels(&frame).await?;

        position = (position + 1) % Gpio16LedStrip::LEN;
        Timer::after_millis(100).await;
    }
}
