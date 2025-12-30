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
use device_kit::mapping::Mapping;
use device_kit::led_strip::Milliamps;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led2d;
use embassy_executor::Spawner;
use embassy_time::Duration;
use panic_probe as _;
use smart_leds::{RGB8, colors};

// Display: 12 wide × 8 tall built from two 12×4 serpentine panels stacked vertically.
const PANEL_12X4: Mapping<48, 4, 12> = Mapping::<48, 4, 12>::serpentine_column_major();
const LED12X8_CUSTOM_MAPPING: Mapping<96, 8, 12> =
    PANEL_12X4.concat_v::<48, 96, 4, 8>(PANEL_12X4);

led2d! {
    pub led12x8,
    pio: PIO1,
    pin: PIN_4,
    dma: DMA_CH1,
    rows: 8,
    cols: 12,
    mapping: LED12X8_CUSTOM_MAPPING,
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
    TestText,
    Santa,
    Clock,
}

impl Mode {
    /// Advance to the next mode in the cycle.
    fn next(self) -> Self {
        match self {
            Self::TestPattern => Self::TestText,
            Self::TestText => Self::Santa,
            Self::Santa => Self::Clock,
            Self::Clock => Self::TestPattern,
        }
    }
}

/// Create a test pattern frame with RGBY corners.
/// Tests all 4 corners and center cross to verify coordinate mapping.
fn create_test_pattern() -> Led12x8Frame {
    let mut frame = Led12x8::new_frame();

    // cmk000 delete Test: columns appear reversed based on GRYB observation (may no longer apply)
    frame[0][0] = colors::RED; // Top-left
    frame[0][Led12x8::COLS - 1] = colors::GREEN; // Top-right
    frame[Led12x8::ROWS - 1][0] = colors::BLUE; // Bottom-left
    frame[Led12x8::ROWS - 1][Led12x8::COLS - 1] = colors::YELLOW; // Bottom-right

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
            Mode::TestText => {
                let mut frame = Led12x8::new_frame();
                led_12x8
                    .write_text_to_frame("HELLO\nWORLD", &[colors::CYAN, colors::MAGENTA], &mut frame)?;
                led_12x8.write_frame(frame).await?;

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
