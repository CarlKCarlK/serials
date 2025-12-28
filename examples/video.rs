//! LED matrix video player - plays a looping 12x8 video at 10 FPS.
//!
//! This example loads 70 pre-encoded frames and displays them in a continuous loop
//! on a 12-wide by 8-tall LED2D display. The display is wired like clock_led8x12.rs
//! but rotated 90 degrees (so 12 columns × 8 rows instead of 8 columns × 12 rows).
//!
//! # Hardware Setup
//!
//! - Two 12x4 LED panels creating a 12x8 display (rotated 90° from clock_led8x12)
//! - LED data on GPIO4
//! - Same physical wiring as clock_led8x12.rs but logically rotated
//!
//! # Converting Your Video to LED Frames
//!
//! The frames are embedded from `target/video_frames_data.rs`, which is generated from
//! your PNG files using:
//!
//! ```bash
//! cargo xtask video-frames-gen 2>/dev/null > target/video_frames_data.rs
//! ```
//!
//! This reads the 65 PNG files from `~/programs/ffmpeg-test/frames12x8_landscape/` and converts
//! them to a Rust array. The generated file contains all frame data as
//! compile-time constants.
//!
//! To use different frames:
//! 1. Replace the PNG files in `~/programs/ffmpeg-test/frames12x8_landscape/`
//! 2. Run the command above to regenerate `target/video_frames_data.rs`
//! 3. Rebuild the example

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::button::{Button, PressedTo};
use device_kit::led_strip::Milliamps;
use device_kit::led2d;
use embassy_executor::Spawner;
use embassy_time::Duration;
use panic_probe as _;
use smart_leds::RGB8;

/// Display mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Show RGBY corner test pattern
    TestPattern,
    /// Play video with no gamma correction (linear)
    Gamma1,
    /// Play video with gamma 2.2 correction
    Gamma2_2,
}

// Display: 12 wide × 8 tall (rotated 90° from clock_led8x12)
// The mapping is the clock_led8x12 mapping but reinterpreted for 12x8 instead of 8x12
led2d! {
    pub led12x8,
    pio: PIO1,
    pin: PIN_4,
    dma: DMA_CH1,
    rows: 8,
    cols: 12,
    mapping: arbitrary([
        // Rotated 90° clockwise from clock_led8x12 mapping
        // Original was 12 rows × 8 cols, now 8 rows × 12 cols
        47, 40, 39, 32, 31, 24, 23, 16, 15, 8, 7, 0,
        46, 41, 38, 33, 30, 25, 22, 17, 14, 9, 6, 1,
        45, 42, 37, 34, 29, 26, 21, 18, 13, 10, 5, 2,
        44, 43, 36, 35, 28, 27, 20, 19, 12, 11, 4, 3,
        95, 88, 87, 80, 79, 72, 71, 64, 63, 56, 55, 48,
        94, 89, 86, 81, 78, 73, 70, 65, 62, 57, 54, 49,
        93, 90, 85, 82, 77, 74, 69, 66, 61, 58, 53, 50,
        92, 91, 84, 83, 76, 75, 68, 67, 60, 59, 52, 51,
    ]),
    max_current: Milliamps(250),
    max_frames: 65,
    font: Font3x4Trim,
}

// Frame data structure: each frame is 8 rows × 12 columns of RGB8 pixels
type VideoFrame = [[RGB8; 12]; 8];

// Total frames in the video
const FRAME_COUNT: usize = 65;

// Frame duration for 10 FPS (100ms per frame)
const FRAME_DURATION: Duration = Duration::from_millis(100);

// Video frames embedded at compile time
// Generated from PNG files using: cargo xtask video-frames-gen
include!("../target/video_frames_data.rs");

/// Gamma 2.2 lookup table for 8-bit values.
/// Pre-computed to avoid floating point math: corrected = (value/255)^2.2 * 255
const GAMMA_2_2_TABLE: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, 10, 11, 11,
    11, 12, 12, 13, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18, 18, 19, 19, 20, 20, 21, 22, 22, 23,
    23, 24, 25, 25, 26, 26, 27, 28, 28, 29, 30, 30, 31, 32, 33, 33, 34, 35, 35, 36, 37, 38, 39, 39,
    40, 41, 42, 43, 43, 44, 45, 46, 47, 48, 49, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61,
    62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 73, 74, 75, 76, 77, 78, 79, 81, 82, 83, 84, 85, 87, 88,
    89, 90, 91, 93, 94, 95, 97, 98, 99, 100, 102, 103, 105, 106, 107, 109, 110, 111, 113, 114, 116,
    117, 119, 120, 121, 123, 124, 126, 127, 129, 130, 132, 133, 135, 137, 138, 140, 141, 143, 145,
    146, 148, 149, 151, 153, 154, 156, 158, 159, 161, 163, 165, 166, 168, 170, 172, 173, 175, 177,
    179, 181, 182, 184, 186, 188, 190, 192, 194, 196, 197, 199, 201, 203, 205, 207, 209, 211, 213,
    215, 217, 219, 221, 223, 225, 227, 229, 231, 234, 236, 238, 240, 242, 244, 246, 248, 251, 253,
    255,
];

/// Apply gamma correction to a single u8 color channel using gamma 2.2.
fn gamma_correct_2_2(value: u8) -> u8 {
    GAMMA_2_2_TABLE[usize::from(value)]
}

/// Apply gamma correction to an RGB8 pixel using gamma 2.2.
fn gamma_correct_rgb_2_2(pixel: RGB8) -> RGB8 {
    RGB8 {
        r: gamma_correct_2_2(pixel.r),
        g: gamma_correct_2_2(pixel.g),
        b: gamma_correct_2_2(pixel.b),
    }
}

/// Apply gamma 2.2 correction to an entire frame.
fn gamma_correct_frame_2_2(frame: &VideoFrame) -> VideoFrame {
    let mut result = *frame;
    for row in &mut result {
        for pixel in row {
            *pixel = gamma_correct_rgb_2_2(*pixel);
        }
    }
    result
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<()> {
    info!("Starting LED matrix video player (12x8 @ 10 FPS)");
    let p = embassy_rp::init(Default::default());

    // Set up the 12x8 LED display on GPIO4
    let led_12x8 = Led12x8::new(p.PIO1, p.DMA_CH1, p.PIN_4, spawner)?;

    // Set up button on GPIO13
    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    info!("Video player initialized");

    // Start in test pattern mode
    let mut mode = Mode::TestPattern;

    loop {
        match mode {
            Mode::TestPattern => {
                // Display test pattern: black background with RGBY dots in corners
                // Note: rows are vertically flipped, so row 7 displays at top, row 0 at bottom
                info!("Mode: Test Pattern - press button to switch to Gamma 1.0");
                let mut test_frame = Led12x8::new_frame();
                // Red - upper left (row 7 = top, col 0 = left)
                test_frame[7][0] = RGB8::new(255, 0, 0);
                // Green - upper right (row 7 = top, col 11 = right)
                test_frame[7][11] = RGB8::new(0, 255, 0);
                // Blue - lower left (row 0 = bottom, col 0 = left)
                test_frame[0][0] = RGB8::new(0, 0, 255);
                // Yellow - lower right (row 0 = bottom, col 11 = right)
                test_frame[0][11] = RGB8::new(255, 255, 0);

                led_12x8.write_frame(test_frame).await?;

                // Wait for button press to switch to next mode
                button.wait_for_press().await;
                mode = Mode::Gamma1;
            }

            Mode::Gamma1 => {
                info!("Mode: Gamma 1.0 (linear) - press button to switch to Gamma 2.2");

                // Convert video frames to Led12x8Frame format with gamma=1.0 (no correction)
                let mut animation_frames = heapless::Vec::<(Led12x8Frame, Duration), 70>::new();
                for video_frame in &VIDEO_FRAMES {
                    let frame = Led12x8Frame::from(*video_frame);
                    animation_frames
                        .push((frame, FRAME_DURATION))
                        .expect("animation frames fit in buffer");
                }

                // Play animation until button press
                led_12x8
                    .animate_until(&animation_frames, async {
                        button.wait_for_press().await;
                    })
                    .await?;
                mode = Mode::Gamma2_2;
            }

            Mode::Gamma2_2 => {
                info!("Mode: Gamma 2.2 - press button to return to test pattern");

                // Convert video frames to Led12x8Frame format with gamma=2.2 correction
                let mut animation_frames = heapless::Vec::<(Led12x8Frame, Duration), 70>::new();
                for video_frame in &VIDEO_FRAMES {
                    let corrected_frame = gamma_correct_frame_2_2(video_frame);
                    let frame = Led12x8Frame::from(corrected_frame);
                    animation_frames
                        .push((frame, FRAME_DURATION))
                        .expect("animation frames fit in buffer");
                }

                // Play animation until button press
                led_12x8
                    .animate_until(&animation_frames, async {
                        button.wait_for_press().await;
                    })
                    .await?;
                mode = Mode::TestPattern;
            }
        }
    }
}
