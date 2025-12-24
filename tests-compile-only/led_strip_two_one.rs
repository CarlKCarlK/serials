#![no_std]
#![no_main]

// cmk000 would be nice to make define_led_strips create 2Ds directly
// cmk000 names of generated modules/sturcts seems a mess and names are inconsistent.

// cmk000 we need to document that `led2d_from_strip` can only be used once
// cmk000 where are are pools? should they be set?

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Milliamps, Rgb, colors};
use device_kit::led2d::led2d_from_strip;
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;

define_led_strips! {
    pio: PIO1,
    strips: [
        g0_strip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_0,
            len: 8,
            max_current: Milliamps(200)
        },
        g3_strip {
            sm: 1,
            dma: DMA_CH1,
            pin: PIN_3,
            len: 48,
            max_current: Milliamps(500)
        }
    ]
}

define_led_strips! {
    pio: PIO0,
    strips: [
        g4_strip {
            sm: 0,
            dma: DMA_CH2,
            pin: PIN_4,
            len: 96,
            max_current: Milliamps(200)
        }
    ]
}

led2d_from_strip! {
    pub led12x4_gpio3,
    strip_type: g3_strip,
    rows: 4,
    cols: 12,
    mapping: serpentine_column_major,
    max_frames: 48,
    font: Font3x4Trim,
}

led2d_from_strip! {
    pub led12x8_gpio4,
    strip_type: g4_strip,
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

async fn inner_main(spawner: Spawner) -> Result<()> {
    let p = embassy_rp::init(Default::default());

    // Shared PIO1: gpio0 (8 LEDs) and gpio3 (12x4 LEDs)
    let (sm0, sm1, _sm2, _sm3) = pio_split!(p.PIO1);
    let strip_gpio0 = g0_strip::new(sm0, p.DMA_CH0, p.PIN_0, spawner)?;
    let strip_gpio3 = g3_strip::new(sm1, p.DMA_CH1, p.PIN_3, spawner)?;
    let led12x4_gpio3 = Led12x4Gpio3::from_strip(strip_gpio3, spawner)?;

    // Single-strip on PIO0: gpio4 (12x8 LEDs = 96)
    let (sm0_pio0, _sm1, _sm2, _sm3) = pio_split!(p.PIO0);
    let strip_gpio4 = g4_strip::new(sm0_pio0, p.DMA_CH2, p.PIN_4, spawner)?;
    let led12x8_gpio4 = Led12x8Gpio4::from_strip(strip_gpio4, spawner)?;

    let go_frame_duration = Duration::from_millis(600);
    let snake_tick = Duration::from_millis(80);

    info!(
        "Running snake on GPIO0 (shared), GOGO on GPIO3 (shared->2D), GOGO on GPIO4 (new_strip->2D)"
    );

    // Snake on gpio0 (shared strip)
    let mut frame_gpio0 = [colors::BLACK; g0_strip::LEN];
    let mut position_gpio0 = 0usize;

    // Prepare two-frame "gogo" animation for gpio3 Led2d
    let mut go_frames_gpio3 = Vec::<_, 2>::new();
    let mut frame1 = Led12x4Gpio3::new_frame();
    led12x4_gpio3.write_text_to_frame(
        "go  ",
        &[
            colors::MAGENTA,
            colors::CYAN,
            colors::ORANGE,
            colors::HOT_PINK,
        ],
        &mut frame1,
    )?;
    go_frames_gpio3
        .push((frame1, go_frame_duration))
        .expect("go_frames has capacity for 2 frames");

    let mut frame2 = Led12x4Gpio3::new_frame();
    led12x4_gpio3.write_text_to_frame(
        "  go",
        &[
            colors::CYAN,
            colors::ORANGE,
            colors::HOT_PINK,
            colors::MAGENTA,
        ],
        &mut frame2,
    )?;
    go_frames_gpio3
        .push((frame2, go_frame_duration))
        .expect("go_frames has capacity for 2 frames");

    // Prepare two-frame "go" animation for gpio4 Led2d
    let mut go_frames_gpio4 = Vec::<_, 2>::new();
    let mut frame1 = Led12x8Gpio4::new_frame();
    led12x8_gpio4.write_text_to_frame(
        "GO\n",
        &[
            colors::MAGENTA,
            colors::CYAN,
            colors::ORANGE,
            colors::HOT_PINK,
        ],
        &mut frame1,
    )?;
    go_frames_gpio4
        .push((frame1, go_frame_duration))
        .expect("go_frames has capacity for 2 frames");

    let mut frame2 = Led12x8Gpio4::new_frame();
    led12x8_gpio4.write_text_to_frame(
        "\nGO",
        &[
            colors::CYAN,
            colors::ORANGE,
            colors::HOT_PINK,
            colors::MAGENTA,
        ],
        &mut frame2,
    )?;
    go_frames_gpio4
        .push((frame2, go_frame_duration))
        .expect("go_frames has capacity for 2 frames");

    // Kick off animations
    led12x4_gpio3.animate(&go_frames_gpio3).await?;
    led12x8_gpio4.animate(&go_frames_gpio4).await?;

    loop {
        step_snake(&mut frame_gpio0, &mut position_gpio0);
        strip_gpio0.update_pixels(&frame_gpio0).await?;
        Timer::after(snake_tick).await;
    }
}

fn step_snake(frame: &mut [Rgb], position: &mut usize) {
    let len = frame.len();
    for color in frame.iter_mut() {
        *color = colors::BLACK;
    }

    for (segment_index, segment_color) in SNAKE_COLORS.iter().enumerate() {
        let segment_position = (position.wrapping_add(segment_index)) % len;
        frame[segment_position] = *segment_color;
    }

    *position = position.wrapping_add(1) % len;
}
