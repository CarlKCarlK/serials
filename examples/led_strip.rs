#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Rgb, colors};
use device_kit::led_strip_simple::Milliamps;
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

define_led_strips! {
    pio: PIO0,
    strips: [
        led_strip0 {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_2,
            len: 8,
            max_current: Milliamps(50)
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

async fn inner_main(spawner: Spawner) -> Result<()> {
    let p = embassy_rp::init(Default::default());

    // Initialize PIO0 bus
    let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO0);

    static LED_STRIP_STATIC: led_strip0::Static = led_strip0::new_static();
    let mut led_strip_0 = led_strip0::new(&LED_STRIP_STATIC, sm0, p.DMA_CH0, p.PIN_2, spawner)?;

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut led_strip_0, hue).await?;

        hue = hue.wrapping_add(3);
        Timer::after_millis(80).await;
    }
}

async fn update_rainbow(strip: &mut led_strip0::Strip, base: u8) -> Result<()> {
    let mut pixels = [colors::BLACK; led_strip0::LEN];
    for idx in 0..led_strip0::LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        pixels[idx] = wheel(offset);
    }
    strip.update_pixels(&pixels).await?;
    Ok(())
}

fn wheel(pos: u8) -> Rgb {
    let pos = 255 - pos;
    if pos < 85 {
        rgb(255 - pos * 3, 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        rgb(0, pos * 3, 255 - pos * 3)
    } else {
        let pos = pos - 170;
        rgb(pos * 3, 255 - pos * 3, 0)
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}
