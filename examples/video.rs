//! LED matrix video player - plays a looping 12x8 video at 10 FPS.
//!
//! This example loads 65 pre-encoded frames and displays them in a continuous loop
//! on a 12-wide by 8-tall LED2D display. The display is wired like clock_led8x12.rs
//! but rotated 90 degrees (so 12 columns × 8 rows instead of 8 columns × 12 rows).
//!
//! The display uses `gamma: Gamma::Gamma2_2` for automatic gamma correction of all
//! frames, which provides more natural perceived brightness on LEDs.
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
use device_kit::led_strip::Milliamps;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led2d;
use embassy_executor::Spawner;
use embassy_time::Duration;
use panic_probe as _;
use smart_leds::RGB8;

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

    info!("Video player initialized - gamma correction applied automatically");

    // Convert video frames to Led12x8Frame format
    let mut animation_frames = heapless::Vec::<(Led12x8Frame, Duration), 70>::new();
    for video_frame in &VIDEO_FRAMES {
        let frame = Led12x8Frame::from(*video_frame);
        animation_frames
            .push((frame, FRAME_DURATION))
            .expect("animation frames fit in buffer");
    }

    // Start animation in background - it will loop forever
    led_12x8.animate(&animation_frames).await?;

    // Keep the task alive
    loop {
        embassy_time::Timer::after(Duration::from_secs(60)).await;
    }
}
