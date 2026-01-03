#![no_std]
#![no_main]

// cmk000 would be nice to make define_led_strips create 2Ds directly
// cmk000 names of generated modules/structs seems a mess and names are inconsistent.

// cmk000 we need to document that `led2d_from_strip` can only be used once
// cmk000 where are are pools? should they be set?

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_layout::LedLayout;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Current, Frame, Rgb, colors};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;

define_led_strips! {
    pio: PIO1,
    LedStripsPio1 {
        gpio0: { pin: PIN_0, len: 8, max_current: Current::Milliamps(200) },
        gpio3: {
            dma: DMA_CH1,
            pin: PIN_3,
            len: 48,
            max_current: Current::Milliamps(500),
            led2d: {
                width: 12,
                height: 4,
                led_layout: LED_LAYOUT_12X4,
                max_frames: 48,
                font: Font3x4Trim,
            }
        }
    }
}

define_led_strips! {
    pio: PIO0,
    LedStripsPio0 {
        gpio4: {
            dma: DMA_CH2,
            pin: PIN_4,
            len: 96,
            max_current: Current::Milliamps(200),
            led2d: {
                width: 8,
                height: 12,
                led_layout: LED_LAYOUT_8X12,
                max_frames: 48,
                font: Font4x6Trim,
            }
        }
    }
}

const SNAKE_LENGTH: usize = 4;
const SNAKE_COLORS: [Rgb; SNAKE_LENGTH] =
    [colors::YELLOW, colors::ORANGE, colors::RED, colors::MAGENTA];

const LED_LAYOUT_12X4: LedLayout<48, 12, 4> = LedLayout::<48, 12, 4>::serpentine_column_major();
const LED_LAYOUT_12X4_ORIENTED: LedLayout<48, 4, 12> =
    LED_LAYOUT_12X4.rotate_cw().flip_h().flip_v();
const LED_LAYOUT_8X12: LedLayout<96, 8, 12> =
    LED_LAYOUT_12X4_ORIENTED.concat_h::<48, 96, 4, 8>(LED_LAYOUT_12X4_ORIENTED);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    if let Err(err) = inner_main(spawner).await {
        panic!("Initialization failed: {:?}", err);
    }
}

async fn inner_main(spawner: Spawner) -> Result<()> {
    let p = embassy_rp::init(Default::default());

    // Shared PIO1: gpio0 (8 LEDs) and gpio3 (12x4 LEDs)
    let (gpio0_led_strip, gpio3_led_strip) =
        LedStripsPio1::new(p.PIO1, p.DMA_CH0, p.PIN_0, p.DMA_CH1, p.PIN_3, spawner)?;
    // Convert gpio3 to led2d
    let gpio3_led_strip = Gpio3LedStripLed2d::from_strip(gpio3_led_strip, spawner)?;

    // Single-strip on PIO0: gpio4 (12x8 LEDs = 96)
    let (gpio4_led_strip,) = LedStripsPio0::new(p.PIO0, p.DMA_CH2, p.PIN_4, spawner)?;
    // Convert gpio4 to led2d
    let gpio4_led_strip = Gpio4LedStripLed2d::from_strip(gpio4_led_strip, spawner)?;

    let go_frame_duration = Duration::from_millis(600);
    let snake_tick = Duration::from_millis(80);

    info!(
        "Running snake on GPIO0 (shared), GOGO on GPIO3 (shared->2D), GOGO on GPIO4 (new_strip->2D)"
    );

    // Snake on gpio0 (shared strip)
    let mut frame_gpio0 = Frame::<{ Gpio0LedStrip::LEN }>::new();
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
        gpio0_led_strip.write_frame(frame_gpio0).await?;
        Timer::after(snake_tick).await;
    }
}

fn step_snake<const N: usize>(frame: &mut Frame<N>, position: &mut usize) {
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
