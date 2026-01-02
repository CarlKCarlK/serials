#![no_std]
#![no_main]

// cmk000 we need to document that `led2d_from_strip` can only be used once
// cmk000 where are are pools? should they be set?

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_layout::LedLayout;
use device_kit::led_strip::Milliamps;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led_strip::{Rgb, colors};
use device_kit::led2d::led2d_from_strip;
use device_kit::pio_split;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;

define_led_strips! {
    pio: PIO1,
    strips: [
        Gpio0LedStrip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_0,
            len: 8,
            max_current: Milliamps(50),
            gamma: Gamma::Linear
        },
        Gpio3LedStrip {
            sm: 1,
            dma: DMA_CH1,
            pin: PIN_3,
            len: 48,
            max_current: Milliamps(500),
            gamma: Gamma::Linear
        },
        Gpio4LedStrip {
            sm: 2,
            dma: DMA_CH2,
            pin: PIN_4,
            len: 48,
            max_current: Milliamps(500),
            gamma: Gamma::Linear
        }
    ]
}

// Shared 12x4 serpentine layout for both LED matrices.
const LED_LAYOUT_12X4: LedLayout<48, 12, 4> = LedLayout::serpentine_column_major();

led2d_from_strip! {
    pub led12x4_gpio3,
    strip_type: Gpio3LedStrip,
    width: 12,
    height: 4,
    led_layout: LED_LAYOUT_12X4,
    max_frames: 48,
    font: Font3x4Trim,
}

led2d_from_strip! {
    pub led12x4_gpio4,
    strip_type: Gpio4LedStrip,
    width: 12,
    height: 4,
    led_layout: LED_LAYOUT_12X4,
    max_frames: 48,
    font: Font3x4Trim,
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
    let (sm0, sm1, sm2, _sm3) = pio_split!(p.PIO1);

    // cmk000 we should have _shared/Shared here
    let gpio0_led_strip = Gpio0LedStrip::new(sm0, p.DMA_CH0, p.PIN_0, spawner)?;
    let gpio3_led_strip = Gpio3LedStrip::new(sm1, p.DMA_CH1, p.PIN_3, spawner)?;
    let gpio4_led_strip = Gpio4LedStrip::new(sm2, p.DMA_CH2, p.PIN_4, spawner)?;

    let led12x4_gpio3 = Led12x4Gpio3::from_strip(gpio3_led_strip, spawner)?;
    let led12x4_gpio4 = Led12x4Gpio4::from_strip(gpio4_led_strip, spawner)?;

    info!("Running snake on GPIO0, GO animations on GPIO3 and GPIO4 (2D)");

    let mut frame_g0 = [colors::BLACK; Gpio0LedStrip::LEN];
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

    led12x4_gpio3.animate(go_frames.clone()).await?;
    led12x4_gpio4.animate(go_frames).await?;

    loop {
        step_snake(&mut frame_g0, &mut pos_g0);
        gpio0_led_strip.update_pixels(&frame_g0).await?;
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
