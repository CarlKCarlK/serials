//! A device abstraction for rectangular LED matrix displays with arbitrary dimensions.
//!
//! See [`Led2d`] for usage details.

use core::convert::Infallible;
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::Vec;
use smart_leds::RGB8;

use crate::Result;

// cmk does this need to be limited and public
/// Maximum frames supported by [`Led2d::animate`].
pub const ANIMATION_MAX_FRAMES: usize = 32;

type Led2dCommandSignal<const N: usize> = Signal<CriticalSectionRawMutex, Command<N>>;
type Led2dCompletionSignal = Signal<CriticalSectionRawMutex, ()>;

#[derive(Clone)]
pub enum Command<const N: usize> {
    DisplayStatic([RGB8; N]),
    Animate(Vec<Frame<N>, ANIMATION_MAX_FRAMES>),
}

/// Frame of animation for [`Led2d::animate`].
#[derive(Clone, Copy, Debug)]
pub struct Frame<const N: usize> {
    pub frame: [RGB8; N],
    pub duration: Duration,
}

impl<const N: usize> Frame<N> {
    #[must_use]
    pub const fn new(frame: [RGB8; N], duration: Duration) -> Self {
        Self { frame, duration }
    }
}

/// Signal resources for [`Led2d`].
pub struct Led2dStatic<const N: usize> {
    command_signal: Led2dCommandSignal<N>,
    completion_signal: Led2dCompletionSignal,
}

impl<const N: usize> Led2dStatic<N> {
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            command_signal: Signal::new(),
            completion_signal: Signal::new(),
        }
    }
}

/// Trait for LED strip drivers that can render a full frame.
pub trait LedStrip<const N: usize> {
    /// Update all pixels at once.
    async fn update_pixels(&mut self, pixels: &[RGB8; N]) -> Result<()>;
}

/// A device abstraction for rectangular LED matrix displays.
///
/// Supports any size display with arbitrary coordinate-to-LED-index mapping.
/// The mapping is stored as a runtime slice, allowing stable Rust without experimental features.
///
/// Rows and columns are metadata used only for indexing - the core type is generic only over N (total LEDs).
pub struct Led2d<'a, const N: usize> {
    command_signal: &'static Led2dCommandSignal<N>,
    completion_signal: &'static Led2dCompletionSignal,
    mapping: &'a [u16],
    cols: usize,
}

impl<'a, const N: usize> Led2d<'a, N> {
    /// Create Led2d device handle.
    ///
    /// The `mapping` slice defines how (column, row) coordinates map to LED strip indices.
    /// Index `row * cols + col` gives the LED index for that position.
    /// Length must equal N (checked with debug_assert).
    #[must_use]
    pub fn new(led2d_static: &'static Led2dStatic<N>, mapping: &'a [u16], cols: usize) -> Self {
        debug_assert_eq!(mapping.len(), N, "mapping length must equal N (total LEDs)");
        Self {
            command_signal: &led2d_static.command_signal,
            completion_signal: &led2d_static.completion_signal,
            mapping,
            cols,
        }
    }

    /// Convert (column, row) coordinates to LED strip index using the stored mapping.
    #[must_use]
    pub fn xy_to_index(&self, column_index: usize, row_index: usize) -> usize {
        self.mapping[row_index * self.cols + column_index] as usize
    }

    /// Render a fully defined frame to the display.
    pub async fn write_frame(&self, frame: [RGB8; N]) -> Result<()> {
        self.command_signal.signal(Command::DisplayStatic(frame));
        self.completion_signal.wait().await;
        Ok(())
    }

    /// Loop through a sequence of animation frames until interrupted by another command.
    pub async fn animate(&self, frames: &[Frame<N>]) -> Result<()> {
        assert!(!frames.is_empty(), "animation requires at least one frame");
        let mut sequence: Vec<Frame<N>, ANIMATION_MAX_FRAMES> = Vec::new();
        for frame in frames {
            assert!(
                frame.duration.as_micros() > 0,
                "animation frame duration must be positive"
            );
            sequence.push(*frame).expect("animation sequence fits");
        }
        self.command_signal.signal(Command::Animate(sequence));
        self.completion_signal.wait().await;
        Ok(())
    }
}

/// Creates a serpentine column-major mapping for rectangular displays.
///
/// Even columns go top-to-bottom (row 0→ROWS-1), odd columns go bottom-to-top (row ROWS-1→0).
/// This matches typical WS2812 LED strip wiring patterns.
///
/// Returns a flat array where index `row * COLS + col` gives the LED index for that position.
#[must_use]
pub const fn serpentine_column_major_mapping<
    const N: usize,
    const ROWS: usize,
    const COLS: usize,
>() -> [u16; N] {
    let mut mapping = [0_u16; N];
    let mut row_index = 0;
    while row_index < ROWS {
        let mut column_index = 0;
        while column_index < COLS {
            let led_index = if column_index % 2 == 0 {
                // Even column: top-to-bottom
                column_index * ROWS + row_index
            } else {
                // Odd column: bottom-to-top
                column_index * ROWS + (ROWS - 1 - row_index)
            };
            mapping[row_index * COLS + column_index] = led_index as u16;
            column_index += 1;
        }
        row_index += 1;
    }
    mapping
}

/// Device loop for Led2d. This is exported so users can create their own task wrappers.
///
/// Since embassy tasks cannot be generic, users must create a concrete wrapper task.
/// Example usage in `led12x4.rs`.
pub async fn led2d_device_loop<const N: usize, S: LedStrip<N>>(
    command_signal: &'static Led2dCommandSignal<N>,
    completion_signal: &'static Led2dCompletionSignal,
    mut strip: S,
) -> Result<Infallible> {
    loop {
        let command = command_signal.wait().await;
        command_signal.reset();

        match command {
            Command::DisplayStatic(frame) => {
                strip.update_pixels(&frame).await?;
                completion_signal.signal(());
            }
            Command::Animate(frames) => {
                let next_command =
                    run_animation_loop(frames, command_signal, completion_signal, &mut strip)
                        .await?;
                match next_command {
                    Command::DisplayStatic(frame) => {
                        strip.update_pixels(&frame).await?;
                        completion_signal.signal(());
                    }
                    Command::Animate(_) => {
                        // Restart animation loop with new sequence
                        continue;
                    }
                }
            }
        }
    }
}

async fn run_animation_loop<const N: usize, S: LedStrip<N>>(
    frames: Vec<Frame<N>, ANIMATION_MAX_FRAMES>,
    command_signal: &'static Led2dCommandSignal<N>,
    completion_signal: &'static Led2dCompletionSignal,
    strip: &mut S,
) -> Result<Command<N>> {
    completion_signal.signal(());

    loop {
        for frame in &frames {
            strip.update_pixels(&frame.frame).await?;

            match select(command_signal.wait(), Timer::after(frame.duration)).await {
                Either::First(new_command) => {
                    command_signal.reset();
                    return Ok(new_command);
                }
                Either::Second(()) => continue,
            }
        }
    }
}
