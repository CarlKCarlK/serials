#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Rgb, colors};
use device_kit::led_strip_simple::Milliamps;
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

define_led_strips! {
    pio: PIO1,
    strips: [
        g0_strip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_0,
            len: 8,
            max_current: Milliamps(50)
        },
        g3_strip {
            sm: 1,
            dma: DMA_CH1,
            pin: PIN_3,
            len: 48,
            max_current: Milliamps(500)
        },
        g4_strip {
            sm: 2,
            dma: DMA_CH2,
            pin: PIN_4,
            len: 96,
            max_current: Milliamps(800)
        }
    ]
}

const SNAKE_LENGTH: usize = 4;
const SNAKE_COLORS: [Rgb; SNAKE_LENGTH] =
    [colors::YELLOW, colors::ORANGE, colors::RED, colors::MAGENTA];

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    if let Err(err) = inner_main(spawner).await {
        panic!("Initialization failed: {:?}", err);
    }
}

async fn inner_main(spawner: Spawner) -> device_kit::Result<()> {
    let p = embassy_rp::init(Default::default());
    let (sm0, sm1, sm2, _sm3) = pio_split!(p.PIO1);

    static G0_STRIP_STATIC: g0_strip::Static = g0_strip::new_static();
    let mut strip_gpio0 = g0_strip::new(&G0_STRIP_STATIC, sm0, p.DMA_CH0, p.PIN_0, spawner)?;

    static G3_STRIP_STATIC: g3_strip::Static = g3_strip::new_static();
    let mut strip_gpio3 = g3_strip::new(&G3_STRIP_STATIC, sm1, p.DMA_CH1, p.PIN_3, spawner)?;

    static G4_STRIP_STATIC: g4_strip::Static = g4_strip::new_static();
    let mut strip_gpio4 = g4_strip::new(&G4_STRIP_STATIC, sm2, p.DMA_CH2, p.PIN_4, spawner)?;

    info!("Running four-segment snakes on three strips (PIO0)");

    let mut frame_g0 = [colors::BLACK; g0_strip::LEN];
    let mut frame_g3 = [colors::BLACK; g3_strip::LEN];
    let mut frame_g4 = [colors::BLACK; g4_strip::LEN];
    let mut pos_g0 = 0usize;
    let mut pos_g3 = 0usize;
    let mut pos_g4 = 0usize;

    loop {
        step_snake(&mut frame_g0, &mut pos_g0);
        step_snake(&mut frame_g3, &mut pos_g3);
        step_snake(&mut frame_g4, &mut pos_g4);

        strip_gpio0.update_pixels(&frame_g0).await?;
        strip_gpio3.update_pixels(&frame_g3).await?;
        strip_gpio4.update_pixels(&frame_g4).await?;

        Timer::after_millis(80).await;
    }
}

fn step_snake(frame: &mut [Rgb], position: &mut usize) {
    let len = frame.len();
    for color in frame.iter_mut() {
        *color = colors::BLACK;
    }

    for (idx, segment_color) in SNAKE_COLORS.iter().enumerate() {
        let pos = (position.wrapping_add(idx)) % len;
        frame[pos] = *segment_color;
    }

    *position = position.wrapping_add(1) % len;
}
