#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Rgb, colors};
use device_kit::led_strip_simple::Milliamps;
use device_kit::led2d::led2d_from_strip;
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

led2d_from_strip! {
    pub led8x12,
    strip_module: g4_strip,
    rows: 12,
    cols: 8,
    mapping: arbitrary([
        47, 46, 45, 44, 95, 94, 93, 92,
        40, 41, 42, 43, 88, 89, 90, 91,
        39, 38, 37, 36, 87, 86, 85, 84,
        32, 33, 34, 35, 80, 81, 82, 83,
        31, 30, 29, 28, 79, 78, 77, 76,
        24, 25, 26, 27, 72, 73, 74, 75,
        23, 22, 21, 20, 71, 70, 69, 68,
        16, 17, 18, 19, 64, 65, 66, 67,
        15, 14, 13, 12, 63, 62, 61, 60,
        8, 9, 10, 11, 56, 57, 58, 59,
        7, 6, 5, 4, 55, 54, 53, 52,
        0, 1, 2, 3, 48, 49, 50, 51,
    ]),
    max_frames: 48,
    font: Font4x6Trim,
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

    let strip_gpio0 = g0_strip::new(sm0, p.DMA_CH0, p.PIN_0, spawner)?;
    let strip_gpio3 = g3_strip::new(sm1, p.DMA_CH1, p.PIN_3, spawner)?;
    let strip_gpio4 = g4_strip::new(sm2, p.DMA_CH2, p.PIN_4, spawner)?;

    static LED8X12_STATIC: Led8x12Static = Led8x12::new_static();
    let led8x12 = Led8x12::from_strip(&LED8X12_STATIC, strip_gpio4, spawner)?;

    info!("Running snakes on GPIO0 and GPIO3, GO animation on GPIO4 (2D)");

    let mut frame_g0 = [colors::BLACK; g0_strip::LEN];
    let mut frame_g3 = [colors::BLACK; g3_strip::LEN];
    let mut pos_g0 = 0usize;
    let mut pos_g3 = 0usize;

    let crazy_colors = [
        [colors::MAGENTA, colors::CYAN],
        [colors::ORANGE, colors::LIME],
        [colors::HOT_PINK, colors::YELLOW],
    ];

    let mut go_frames = heapless::Vec::<_, 6>::new();
    for color_set in &crazy_colors {
        let mut frame_top = Led8x12::new_frame();
        led8x12.write_text_to_frame("GO\n  ", color_set, &mut frame_top)?;
        go_frames
            .push((frame_top, embassy_time::Duration::from_millis(1000)))
            .map_err(|_| device_kit::Error::FormatError)?;

        let mut frame_bottom = Led8x12::new_frame();
        led8x12.write_text_to_frame("  \nGO", color_set, &mut frame_bottom)?;
        go_frames
            .push((frame_bottom, embassy_time::Duration::from_millis(1000)))
            .map_err(|_| device_kit::Error::FormatError)?;
    }

    led8x12.animate(&go_frames).await?;

    loop {
        step_snake(&mut frame_g0, &mut pos_g0);
        step_snake(&mut frame_g3, &mut pos_g3);

        strip_gpio0.update_pixels(&frame_g0).await?;
        strip_gpio3.update_pixels(&frame_g3).await?;

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
