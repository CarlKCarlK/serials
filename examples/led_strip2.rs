#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use lib::{define_led_strip, Result};
use smart_leds::RGB8;
use panic_probe as _;

// WS2812B 4x12 LED matrix (48 pixels)
// Uses PIO1, State Machine 0, DMA_CH1, GPIO16 (pin 21)
define_led_strip! {
    led_strip2 as LedStrip2 {
        task: led_strip_2_driver,
        pio: PIO1,
        irq: PIO1_IRQ_0,
        sm: { field: sm0, index: 0 },
        dma: DMA_CH1,
        pin: PIN_16,
        len: 48
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
    info!("Wiring: Red->VSYS(pin39), White->GND(pin38), Green->GPIO16(pin21)");

    let mut position: usize = 0;

    loop {
        update_snake(&mut led_strip, position)
            .await
            .expect("pattern update failed");

        position = (position + 1) % LedStrip2::LEN;
        Timer::after_millis(100).await;
    }
}

async fn update_snake(strip: &mut LedStrip2::Strip, head_pos: usize) -> Result<()> {
    const SNAKE_LENGTH: usize = 5;
    const SNAKE_COLOR: RGB8 = RGB8::new(25, 25, 25); // 10% white
    const BLACK: RGB8 = RGB8::new(0, 0, 0);

    // Turn off all LEDs
    for idx in 0..LedStrip2::LEN {
        strip.update_pixel(idx, BLACK).await?;
    }

    // Light up the snake (5 pixels)
    for i in 0..SNAKE_LENGTH {
        let pos = (head_pos + LedStrip2::LEN - i) % LedStrip2::LEN;
        strip.update_pixel(pos, SNAKE_COLOR).await?;
    }
    
    Ok(())
}
