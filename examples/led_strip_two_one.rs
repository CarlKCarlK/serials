#![no_std]
#![no_main]

// cmk000 we need to document that `led2d_from_strip` can only be used once
// cmk000 where are are pools? should they be set?

use defmt::info;
use defmt_rtt as _;
use device_kit::led2d::led2d_from_strip;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{LedStripStatic, Milliamps, Rgb, colors, new_strip};
use device_kit::pio_split;
use device_kit::Result;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;

define_led_strips! {
    pio: PIO1,
    strips: [
        g3_strip {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_3,
            len: 48,
            max_current: Milliamps(500)
        },
        g4_strip {
            sm: 1,
            dma: DMA_CH1,
            pin: PIN_4,
            len: 48,
            max_current: Milliamps(500)
        }
    ]
}

led2d_from_strip! {
    pub led12x4_gpio3,
    strip_module: g3_strip,
    rows: 4,
    cols: 12,
    mapping: serpentine_column_major,
    max_frames: 48,
    font: Font3x4Trim,
}

led2d_from_strip! {
    pub led12x4_gpio4,
    strip_module: g4_strip,
    rows: 4,
    cols: 12,
    mapping: serpentine_column_major,
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

    const MAX_CURRENT_SIMPLE: Milliamps = Milliamps(50);

    type StripStaticGpio0 = LedStripStatic<8>;
    static STRIP_STATIC_GPIO0: StripStaticGpio0 = StripStaticGpio0::new_static();
    let mut strip_gpio0 = new_strip!(
        &STRIP_STATIC_GPIO0, // static resources
        PIN_0,               // data pin
        p.PIO0,              // PIO block for the simple strip
        MAX_CURRENT_SIMPLE   // max current budget (mA)
    )
    .await;

    let (sm0, sm1, _sm2, _sm3) = pio_split!(p.PIO1);

    let strip_gpio3 = g3_strip::new(sm0, p.DMA_CH0, p.PIN_3, spawner)?;
    let strip_gpio4 = g4_strip::new(sm1, p.DMA_CH1, p.PIN_4, spawner)?;

    let led12x4_gpio3 = Led12x4Gpio3::from_strip(strip_gpio3, spawner)?;
    let led12x4_gpio4 = Led12x4Gpio4::from_strip(strip_gpio4, spawner)?;

    info!(
        "Running snake on GPIO0 (simple strip), GO animations on GPIO3 and GPIO4 (shared PIO1)"
    );

    let mut frame_gpio0 = [colors::BLACK; StripStaticGpio0::LEN];
    let mut position_gpio0 = 0usize;

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

    led12x4_gpio3.animate(&go_frames).await?;
    led12x4_gpio4.animate(&go_frames).await?;

    loop {
        step_snake(&mut frame_gpio0, &mut position_gpio0);
        strip_gpio0.update_pixels(&frame_gpio0).await?;
        Timer::after_millis(80).await;
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
