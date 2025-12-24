#![no_std]
#![no_main]

// cmk000 we need to document that `led2d_from_strip` can only be used once
// cmk000 where are are pools? should they be set?

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{LedStripStatic, Milliamps, Rgb, colors, new_strip};
use device_kit::led2d::{
    Frame, Led2dFont, led2d_from_strip, render_text_to_frame, serpentine_column_major_mapping,
};
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

led2d_from_strip! {
    pub led12x4_gpio3,
    strip_module: g3_strip,
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

    const MAX_CURRENT_SIMPLE: Milliamps = Milliamps(200);

    // Shared PIO1: gpio0 (8 LEDs) and gpio3 (12x4 LEDs)
    let (sm0, sm1, _sm2, _sm3) = pio_split!(p.PIO1);
    let strip_gpio0 = g0_strip::new(sm0, p.DMA_CH0, p.PIN_0, spawner)?;
    let strip_gpio3 = g3_strip::new(sm1, p.DMA_CH1, p.PIN_3, spawner)?;
    let led12x4_gpio3 = Led12x4Gpio3::from_strip(strip_gpio3, spawner)?;

    // Single-strip new_strip! on PIO0/SM0/DMA_CH2: gpio4 (8x12 LEDs = 96)
    type StripStaticGpio4 = LedStripStatic<96>;
    static STRIP_STATIC_GPIO4: StripStaticGpio4 = StripStaticGpio4::new_static();
    let mut strip_gpio4 = new_strip!(
        &STRIP_STATIC_GPIO4,
        PIN_4,
        p.PIO0,
        DMA_CH2,
        MAX_CURRENT_SIMPLE
    )
    .await;

    // Convert gpio4 new_strip! into a 2D helper by manually mapping frames.
    const ROWS_GPIO4: usize = 8;
    const COLS_GPIO4: usize = 12;
    const N_GPIO4: usize = ROWS_GPIO4 * COLS_GPIO4;
    const MAPPING_GPIO4: [u16; N_GPIO4] =
        serpentine_column_major_mapping::<N_GPIO4, ROWS_GPIO4, COLS_GPIO4>();
    let go_frame_duration = Duration::from_millis(600);
    let snake_tick = Duration::from_millis(80);
    let font = Led2dFont::Font3x4Trim.to_font();

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
        "gogo",
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
        " ogo",
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

    // Prepare two-frame "gogo" animation for gpio4 (manual 2D -> 1D mapping)
    let mut go_frames_gpio4 = [Frame::<ROWS_GPIO4, COLS_GPIO4>::new(); 2];
    render_text_to_frame(
        &mut go_frames_gpio4[0],
        &font,
        "gogo",
        &[
            colors::MAGENTA,
            colors::CYAN,
            colors::ORANGE,
            colors::HOT_PINK,
        ],
        (0, 0),
    )?;
    render_text_to_frame(
        &mut go_frames_gpio4[1],
        &font,
        " ogo",
        &[
            colors::CYAN,
            colors::ORANGE,
            colors::HOT_PINK,
            colors::MAGENTA,
        ],
        (0, 0),
    )?;

    let go_frames_gpio4 = [
        (
            flatten_frame(go_frames_gpio4[0], &MAPPING_GPIO4),
            Duration::from_millis(600),
        ),
        (
            flatten_frame(go_frames_gpio4[1], &MAPPING_GPIO4),
            Duration::from_millis(600),
        ),
    ];

    // Kick off animations
    led12x4_gpio3.animate(&go_frames_gpio3).await?;

    loop {
        for (pixels, frame_duration) in &go_frames_gpio4 {
            strip_gpio4.update_pixels(pixels).await?;

            let mut elapsed = Duration::from_millis(0);
            while elapsed < *frame_duration {
                step_snake(&mut frame_gpio0, &mut position_gpio0);
                strip_gpio0.update_pixels(&frame_gpio0).await?;

                let remaining = *frame_duration - elapsed;
                let sleep = if remaining > snake_tick {
                    snake_tick
                } else {
                    remaining
                };

                Timer::after(sleep).await;
                elapsed += sleep;
            }
        }
    }
}

fn flatten_frame<const ROWS: usize, const COLS: usize, const N: usize>(
    frame: Frame<ROWS, COLS>,
    mapping: &[u16; N],
) -> [Rgb; N] {
    let mut out = [colors::BLACK; N];
    for row in 0..ROWS {
        for col in 0..COLS {
            let idx = mapping[row * COLS + col] as usize;
            out[idx] = frame[row][col];
        }
    }
    out
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
