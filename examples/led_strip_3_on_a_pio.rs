#![no_std]
#![no_main]

// cmk000 we need to document that `led2d_from_strip` can only be used once
// cmk000 where are are pools? should they be set?

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_layout::LedLayout;
use device_kit::led_strip::led_strips;
use device_kit::led_strip::{Frame, Rgb, colors};
use device_kit::led2d::led2d_from_strip;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;

led_strips! {
    LedStrips {
        gpio0: { pin: PIN_0, len: 8},
        gpio3: { pin: PIN_3, len: 48},
        gpio4: { pin: PIN_4, len: 96}
    }
}

const LED_LAYOUT_12X4: LedLayout<48, 12, 4> = LedLayout::serpentine_column_major();
const LED_LAYOUT_8X12: LedLayout<96, 8, 12> = LED_LAYOUT_12X4.concat_v(LED_LAYOUT_12X4).rotate_cw();

led2d_from_strip! {
    pub led12x4_gpio3,
    strip_type: Gpio3LedStrip,
    width: 12,
    height: 4,
    led_layout: LED_LAYOUT_12X4,
    max_frames: 2,
    font: Font3x4Trim,
}

led2d_from_strip! {
    pub led8x12_gpio4,
    strip_type: Gpio4LedStrip,
    width: 8,
    height: 12,
    led_layout: LED_LAYOUT_8X12,
    max_frames: 2,
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

    let (gpio0_led_strip, gpio3_led_strip, gpio4_led_strip) = LedStrips::new(
        p.PIO0, p.DMA_CH0, p.PIN_0, p.DMA_CH1, p.PIN_3, p.DMA_CH2, p.PIN_4, spawner,
    )?;

    let led12x4_gpio3 = Led12x4Gpio3::from_strip(gpio3_led_strip, spawner)?;
    let led8x12_gpio4 = Led8x12Gpio4::from_strip(gpio4_led_strip, spawner)?;

    info!("Running snake on GPIO0, GO animations on GPIO3 (12x4) and GPIO4 (8x12 rotated)");

    let mut frame_g0 = Frame::<{ Gpio0LedStrip::LEN }>::new();
    let mut pos_g0 = 0usize;

    // Create animation frames: "go  " and "  go" with unique colors per character
    let mut go_frames = Vec::<_, 2>::new();

    // Frame 1: "go  " - each character gets its own color
    let mut frame1 = Led12x4Gpio3::new_frame();
    led12x4_gpio3.write_text_to_frame(
        "go  ",
        &[colors::MAGENTA, colors::CYAN, colors::BLACK, colors::BLACK],
        &mut frame1,
    )?;
    go_frames
        .push((frame1, Duration::from_millis(1000)))
        .expect("go_frames has capacity for 2 frames");

    // Frame 2: "  go" - each character gets its own color
    let mut frame2 = Led12x4Gpio3::new_frame();
    led12x4_gpio3.write_text_to_frame(
        "  go",
        &[
            colors::BLACK,
            colors::BLACK,
            colors::ORANGE,
            colors::HOT_PINK,
        ],
        &mut frame2,
    )?;
    go_frames
        .push((frame2, Duration::from_millis(1000)))
        .expect("go_frames has capacity for 2 frames");

    led12x4_gpio3.animate(go_frames).await?;

    // Create separate animation for the 8x12 rotated display with 2-line text
    let mut go_frames_8x12 = Vec::<_, 2>::new();

    // Frame 1: "GO\n  " - two lines
    let mut frame1_8x12 = Led8x12Gpio4::new_frame();
    led8x12_gpio4.write_text_to_frame(
        "GO\n  ",
        &[colors::MAGENTA, colors::CYAN, colors::BLACK, colors::BLACK],
        &mut frame1_8x12,
    )?;
    go_frames_8x12
        .push((frame1_8x12, Duration::from_millis(1000)))
        .expect("go_frames_8x12 has capacity for 2 frames");

    // Frame 2: "  \nGO" - two lines
    let mut frame2_8x12 = Led8x12Gpio4::new_frame();
    led8x12_gpio4.write_text_to_frame(
        "  \nGO",
        &[
            colors::BLACK,
            colors::BLACK,
            colors::ORANGE,
            colors::HOT_PINK,
        ],
        &mut frame2_8x12,
    )?;
    go_frames_8x12
        .push((frame2_8x12, Duration::from_millis(1000)))
        .expect("go_frames_8x12 has capacity for 2 frames");

    led8x12_gpio4.animate(go_frames_8x12).await?;

    loop {
        step_snake(&mut frame_g0, &mut pos_g0);
        gpio0_led_strip.write_frame(frame_g0).await?;
        Timer::after_millis(80).await;
    }
}

fn step_snake<const N: usize>(frame: &mut Frame<N>, position: &mut usize) {
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
