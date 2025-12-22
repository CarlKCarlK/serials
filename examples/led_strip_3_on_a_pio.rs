#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Rgb, colors};
use device_kit::led_strip_simple::Milliamps;

define_led_strips! {
    pio: PIO0,
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
const SNAKE_COLORS: [Rgb; SNAKE_LENGTH] = [
    colors::YELLOW,
    colors::ORANGE,
    colors::RED,
    colors::MAGENTA,
];

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());
    let (pio_bus, sm0, sm1, sm2, _sm3) = pio0_split(p.PIO0);

    static G0_STRIP_STATIC: g0_strip::Static = g0_strip::new_static();
    static G3_STRIP_STATIC: g3_strip::Static = g3_strip::new_static();
    static G4_STRIP_STATIC: g4_strip::Static = g4_strip::new_static();

    let mut strip_gpio0 = g0_strip::new(
        spawner,
        &G0_STRIP_STATIC,
        pio_bus,
        sm0,
        p.DMA_CH0.into(),
        p.PIN_0.into(),
    )
    .expect("failed to start GPIO0 strip");

    let mut strip_gpio3 = g3_strip::new(
        spawner,
        &G3_STRIP_STATIC,
        pio_bus,
        sm1,
        p.DMA_CH1.into(),
        p.PIN_3.into(),
    )
    .expect("failed to start GPIO3 strip");

    let mut strip_gpio4 = g4_strip::new(
        spawner,
        &G4_STRIP_STATIC,
        pio_bus,
        sm2,
        p.DMA_CH2.into(),
        p.PIN_4.into(),
    )
    .expect("failed to start GPIO4 strip");

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

        strip_gpio0
            .update_pixels(&frame_g0)
            .await
            .expect("update g0 failed");
        strip_gpio3
            .update_pixels(&frame_g3)
            .await
            .expect("update g3 failed");
        strip_gpio4
            .update_pixels(&frame_g4)
            .await
            .expect("update g4 failed");

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
