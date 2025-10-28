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
    info!("⚠️  Keep brightness low! 48 LEDs at full white = ~2.9A");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut led_strip, hue)
            .await
            .expect("pattern update failed");

        hue = hue.wrapping_add(2);
        Timer::after_millis(50).await;
    }
}

async fn update_rainbow(strip: &mut LedStrip2::Strip, base: u8) -> Result<()> {
    // Create rainbow across the 4x12 matrix
    for idx in 0..LedStrip2::LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(5));
        strip.update_pixel(idx, wheel(offset)).await?;
    }
    Ok(())
}

fn wheel(pos: u8) -> RGB8 {
    let pos = 255 - pos;
    let (r, g, b) = if pos < 85 {
        (255 - pos * 3, 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        (0, pos * 3, 255 - pos * 3)
    } else {
        let pos = pos - 170;
        (pos * 3, 255 - pos * 3, 0)
    };
    // Scale to 10% brightness
    RGB8::new(r / 10, g / 10, b / 10)
}
