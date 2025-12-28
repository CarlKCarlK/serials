//! LED matrix video player - plays looping 12x8 videos with button-controlled mode switching.
//!
//! This example cycles through multiple display modes using a button:
//! 1. **Test Pattern**: RGBY corners (Red top-left, Green top-right, Blue bottom-left, Yellow bottom-right)
//! 2. **Santa**: 65-frame video at 10 FPS
//! 3. **Cat**: Video converted from user's camera roll (when generated)
//!
//! Press the button at any time to advance to the next mode.
//!
//! # Hardware Setup
//!
//! - Two 12x4 LED panels creating a 12x8 display (rotated 90° from clock_led8x12)
//! - LED data on GPIO4
//! - Button on GPIO13 (wired to ground)
//! - Same physical wiring as clock_led8x12.rs but logically rotated
//!
//! # Converting Your Video to LED Frames
//!
//! ## Santa Video (Pre-configured)
//!
//! The santa frames are embedded from `video_frames_data.rs`, which is **auto-generated**
//! during the build process from PNG files in `~/programs/ffmpeg-test/frames12x8_landscape/`.
//!
//! The build system automatically:
//! 1. Detects when building the `video` example
//! 2. Runs `cargo xtask video-frames-gen` to convert 65 PNG files to Rust code
//! 3. Writes the result to `video_frames_data.rs` in the crate root
//! 4. Includes it at compile time
//!
//! To use different frames:
//! 1. Replace the PNG files in `~/programs/ffmpeg-test/frames12x8_landscape/`
//! 2. Delete `video_frames_data.rs` to force regeneration (or run `cargo clean`)
//! 3. Rebuild the example - frames will be regenerated automatically
//!
//! Manual generation:
//! ```bash
//! cargo xtask video-frames-gen > video_frames_data.rs
//! ```
//!
//! ## Cat Video (From Video File)
//!
//! To add the cat video mode:
//! 1. Place your video file at: `C:\Users\carlk\OneDrive\SkyDrive camera roll\cat.mp4`
//!    (or update the path in `xtask/src/video_frames_gen.rs`)
//! 2. Generate frames:
//!    ```bash
//!    cargo xtask cat-frames-gen > cat_frames_data.rs
//!    ```
//! 3. Uncomment the cat-related lines in this file:
//!    - `include!("../cat_frames_data.rs");`
//!    - `Mode::Cat` enum variant
//!    - Cat playback logic in the match statement
//!
//! The xtask command uses ffmpeg to:
//! - Extract frames at 10 FPS
//! - Scale to 12x8 pixels
//! - Convert to embedded Rust arrays

#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::button::{Button, PressedTo};
use device_kit::led_strip::Milliamps;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led2d;
use embassy_executor::Spawner;
use embassy_time::Duration;
use panic_probe as _;
use smart_leds::{RGB8, colors};

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
    gamma: Gamma::Gamma2_2,
    max_frames: 70,
    font: Font3x4Trim,
}

// Total frames in the video
// Now defined in generated files: SANTA_FRAME_COUNT, CAT_FRAME_COUNT (with per-frame durations)

// Video frames and frame duration embedded at compile time
// Auto-generated during build from PNG files in ~/programs/ffmpeg-test/frames12x8_landscape/
// See build.rs for generation logic
include!("../video_frames_data.rs");

// Cat video frames - generated from OneDrive camera roll
// include!("../cat_frames_data.rs");

// Hand video frames - generated from OneDrive camera roll
// include!("../hand_frames_data.rs");

// Clock video frames
include!("../clock_frames_data.rs");

/// Video display modes.
#[derive(defmt::Format, Clone, Copy)]
enum Mode {
    TestPattern,
    Santa,
    Clock,
}

impl Mode {
    /// Advance to the next mode in the cycle.
    fn next(self) -> Self {
        match self {
            Self::TestPattern => Self::Santa,
            Self::Santa => Self::Clock,
            Self::Clock => Self::TestPattern,
        }
    }
}

/// Create a test pattern frame with RGBY corners.
/// Tests all 4 corners and center cross to verify coordinate mapping.
fn create_test_pattern() -> Led12x8Frame {
    let mut frame = Led12x8::new_frame();

    // Test: columns appear reversed based on GRYB observation
    frame[0][Led12x8::COLS - 1] = colors::RED; // Top-left (reversed col)
    frame[0][0] = colors::GREEN; // Top-right (reversed col)
    frame[Led12x8::ROWS - 1][Led12x8::COLS - 1] = colors::BLUE; // Bottom-left (reversed col)
    frame[Led12x8::ROWS - 1][0] = colors::YELLOW; // Bottom-right (reversed col)

    // Center cross for additional verification
    frame[Led12x8::ROWS / 2][Led12x8::COLS / 2] = colors::WHITE;

    frame.into()
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<()> {
    info!("Starting LED matrix video player (12x8 @ 10 FPS with gamma 2.2)");
    let p = embassy_rp::init(Default::default());

    // Set up the 12x8 LED display on GPIO4 with gamma 2.2 correction
    let led_12x8 = Led12x8::new(p.PIO1, p.DMA_CH1, p.PIN_4, spawner)?;

    // Set up button on GPIO13 (wired to ground)
    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    info!("Video player initialized - gamma correction applied automatically");

    let mut mode = Mode::TestPattern;

    loop {
        info!("Entering mode: {:?}", mode);

        match mode {
            Mode::TestPattern => {
                let test_pattern = create_test_pattern();
                led_12x8.write_frame(test_pattern).await?;

                button.wait_for_press_duration().await;
                mode = mode.next();
            }
            Mode::Santa => {
                let frames_with_duration = SANTA_FRAMES
                    .iter()
                    .map(|&(frame, duration)| (frame.into(), duration));
                led_12x8.animate(frames_with_duration).await?;
                button.wait_for_press_duration().await;
                mode = mode.next();
            }
            Mode::Clock => {
                let frames_with_duration = CLOCK_FRAMES
                    .iter()
                    .map(|&(frame, duration)| (frame.into(), duration));
                led_12x8.animate(frames_with_duration).await?;
                button.wait_for_press_duration().await;
                mode = mode.next();
            }
        }
    }
}
