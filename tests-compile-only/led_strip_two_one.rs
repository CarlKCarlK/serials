#![no_std]
#![no_main]

// cmk000 would be nice to make define_led_strips_shared create 2Ds directly
// cmk000 names of generated modules/structs seems a mess and names are inconsistent.

// cmk000 we need to document that `led2d_from_strip` can only be used once
// cmk000 where are are pools? should they be set?

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips_shared;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led_strip::{Milliamps, Rgb, colors};
use device_kit::led_layout::LedLayout;
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;

define_led_strips_shared! {
    pio: PIO1,
    strips: [
        Gpio0LedStrip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_0,
            len: 8,
            max_current: Milliamps(200),
            gamma: Gamma::Linear
        },
        Gpio3LedStrip {
            sm: 1,
            dma: DMA_CH1,
            pin: PIN_3,
            len: 48,
            max_current: Milliamps(500),
            gamma: Gamma::Linear,
            led2d: {
                rows: 4,
                cols: 12,
                mapping: serpentine_column_major,
                max_frames: 48,
                font: Font3x4Trim,
            }
        }
    ]
}

define_led_strips_shared! {
    pio: PIO0,
    strips: [
        Gpio4LedStrip {
            sm: 0,
            dma: DMA_CH2,
            pin: PIN_4,
            len: 96,
            max_current: Milliamps(200),
            gamma: Gamma::Linear,
            led2d: {
                rows: 12,
                cols: 8,
                mapping: LED8X12_MAPPING,
                max_frames: 48,
                font: Font4x6Trim,
            }
        }
    ]
}

const SNAKE_LENGTH: usize = 4;
const SNAKE_COLORS: [Rgb; SNAKE_LENGTH] =
    [colors::YELLOW, colors::ORANGE, colors::RED, colors::MAGENTA];

const PANEL_12X4: LedLayout<48, 4, 12> = LedLayout::<48, 4, 12>::serpentine_column_major();
const PANEL_12X4_ORIENTED: LedLayout<48, 12, 4> = PANEL_12X4.rotate_cw().flip_h().flip_v();
const LED8X12_MAPPING: LedLayout<96, 12, 8> =
    PANEL_12X4_ORIENTED.concat_h::<48, 96, 4, 8>(PANEL_12X4_ORIENTED);

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
    let gpio0_led_strip = Gpio0LedStrip::new(sm0, p.DMA_CH0, p.PIN_0, spawner)?;
    let gpio3_led_strip = Gpio3LedStrip::new_led2d(sm1, p.DMA_CH1, p.PIN_3, spawner)?;

    // Single-strip on PIO0: gpio4 (12x8 LEDs = 96)
    let (sm0_pio0, _sm1, _sm2, _sm3) = pio_split!(p.PIO0);
    let gpio4_led_strip = Gpio4LedStrip::new_led2d(sm0_pio0, p.DMA_CH2, p.PIN_4, spawner)?;

    let go_frame_duration = Duration::from_millis(600);
    let snake_tick = Duration::from_millis(80);

    info!(
        "Running snake on GPIO0 (shared), GOGO on GPIO3 (shared->2D), GOGO on GPIO4 (new_strip->2D)"
    );

    // Snake on gpio0 (shared strip)
    let mut frame_gpio0 = [colors::BLACK; Gpio0LedStrip::LEN];
    let mut position_gpio0 = 0usize;

    // Prepare two-frame "gogo" animation for gpio3 Led2d
    let mut go_frames_gpio3 = Vec::<_, 2>::new();
    let mut frame1 = Gpio3LedStripLed2d::new_frame();
    gpio3_led_strip.write_text_to_frame(
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

    let mut frame2 = Gpio3LedStripLed2d::new_frame();
    gpio3_led_strip.write_text_to_frame(
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
    let mut frame1 = Gpio4LedStripLed2d::new_frame();
    gpio4_led_strip.write_text_to_frame(
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

    let mut frame2 = Gpio4LedStripLed2d::new_frame();
    gpio4_led_strip.write_text_to_frame(
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
    gpio3_led_strip.animate(go_frames_gpio3.clone()).await?;
    gpio4_led_strip.animate(go_frames_gpio4).await?;

    loop {
        step_snake(&mut frame_gpio0, &mut position_gpio0);
        gpio0_led_strip.update_pixels(&frame_gpio0).await?;
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
