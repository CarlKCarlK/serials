//! A device abstraction for rectangular LED matrix displays with arbitrary dimensions.
//!
//! See [`Led2d`] for usage details.

// Re-export for macro use
pub use paste;

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

pub type Led2dCommandSignal<const N: usize> = Signal<CriticalSectionRawMutex, Command<N>>;
pub type Led2dCompletionSignal = Signal<CriticalSectionRawMutex, ()>;

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
    pub command_signal: Led2dCommandSignal<N>,
    pub completion_signal: Led2dCompletionSignal,
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

/// Declares an Embassy task that runs [`led2d_device_loop`] for a concrete LED strip type.
///
/// Each `Led2d` device needs a monomorphic task because `#[embassy_executor::task]` does not
/// support generics. This macro generates the boilerplate wrapper and keeps your modules tidy.
///
/// # Example
/// ```ignore
/// # #![no_std]
/// # use panic_probe as _;
/// use embassy_executor::Spawner;
/// use embassy_rp::{init, peripherals::PIO1};
/// use serials::Result;
/// use serials::led2d::{Led2dStatic, led2d_device_task};
/// use serials::led_strip_simple::{LedStripSimple, LedStripSimpleStatic, Milliamps};
///
/// const COLS: usize = 12;
/// const ROWS: usize = 4;
/// const N: usize = COLS * ROWS;
///
#[macro_export]
macro_rules! led2d_device_task {
    (
        $task_name:ident,
        $strip_ty:ty,
        $n:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!(
            @inner
            ()
            $task_name,
            $strip_ty,
            $n
        );
    };
    (
        $vis:vis $task_name:ident,
        $strip_ty:ty,
        $n:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!(
            @inner
            ($vis)
            $task_name,
            $strip_ty,
            $n
        );
    };
    (
        @inner
        ($($vis:tt)*)
        $task_name:ident,
        $strip_ty:ty,
        $n:expr $(,)?
    ) => {
        #[embassy_executor::task]
        $($vis)* async fn $task_name(
            command_signal: &'static $crate::led2d::Led2dCommandSignal<$n>,
            completion_signal: &'static $crate::led2d::Led2dCompletionSignal,
            strip: $strip_ty,
        ) {
            let err = $crate::led2d::led2d_device_loop(command_signal, completion_signal, strip)
                .await
                .unwrap_err();
            panic!("{err}");
        }
    };
}

pub use led2d_device_task;

/// Declares the full Led2d device/static pair plus the background task wrapper.
///
/// This extends [`led2d_device_task!`] by also generating a static resource holder with
/// `new_static`/`new` so callers do not need to wire up the signals and task spawning manually.
///
/// # Example
/// ```ignore
/// # #![no_std]
/// # use panic_probe as _;
/// use defmt::info;
/// use embassy_executor::Spawner;
/// use embassy_rp::{init, peripherals::PIO1};
/// use serials::Result;
/// use serials::led2d::{Led2d, led2d_device};
/// use serials::led_strip_simple::{LedStripSimple, LedStripSimpleStatic, Milliamps};
///
/// const COLS: usize = 12;
/// const ROWS: usize = 4;
/// const N: usize = COLS * ROWS;
/// const MAPPING: [u16; N] = serials::led2d::serpentine_column_major_mapping::<N, ROWS, COLS>();
///
#[macro_export]
macro_rules! led2d_device {
    (
        $vis:vis struct $resources_name:ident,
        task: $task_vis:vis $task_name:ident,
        strip: $strip_ty:ty,
        leds: $n:expr,
        mapping: $mapping:expr,
        cols: $cols:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!($task_vis $task_name, $strip_ty, $n);

        $vis struct $resources_name {
            led2d_static: $crate::led2d::Led2dStatic<$n>,
        }

        impl $resources_name {
            /// Create the static resources for this Led2d instance.
            #[must_use]
            pub const fn new_static() -> Self {
                Self {
                    led2d_static: $crate::led2d::Led2dStatic::new_static(),
                }
            }

            /// Construct the `Led2d` handle, spawning the background task automatically.
            pub fn new(
                &'static self,
                strip: $strip_ty,
                spawner: ::embassy_executor::Spawner,
            ) -> $crate::Result<$crate::led2d::Led2d<'static, $n>> {
                let token = $task_name(
                    &self.led2d_static.command_signal,
                    &self.led2d_static.completion_signal,
                    strip,
                )?;
                spawner.spawn(token);
                Ok($crate::led2d::Led2d::new(
                    &self.led2d_static,
                    $mapping,
                    $cols,
                ))
            }
        }
    };
}

pub use led2d_device;

/// Declares a complete Led2d device abstraction with LedStripSimple integration.
///
/// This macro generates all the boilerplate for a rectangular LED matrix device:
/// - Constants: ROWS, COLS, N (total LEDs)
/// - Mapping array (serpentine column-major by default)
/// - Static struct with embedded LedStripSimple resources
/// - Device struct with Led2d handle
/// - Constructor that creates strip + spawns task
/// - Wrapper methods: write_frame, animate, xy_to_index
///
/// # Example
///
/// ```ignore
/// use serials::led2d::led2d_device_simple;
///
/// led2d_device_simple! {
///     pub led12x4,
///     rows: 4,
///     cols: 12,
///     pio: PIO1,
/// }
///
/// // Generates:
/// // pub struct Led12x4 { ... }
/// // pub struct Led12x4Static { ... }
/// // pub const ROWS: usize = 4;
/// // pub const COLS: usize = 12;
/// // pub const N: usize = 48;
/// // const MAPPING: [u16; 48] = ...;
/// // impl Led12x4 {
/// //     pub const fn new_static() -> Led12x4Static { ... }
/// //     pub async fn new(...) -> Result<Self> { ... }
/// //     pub async fn write_frame(...) -> Result<()> { ... }
/// //     pub async fn animate(...) -> Result<()> { ... }
/// //     pub fn xy_to_index(...) -> usize { ... }
/// // }
/// ```
#[macro_export]
macro_rules! led2d_device_simple {
    (
        $vis:vis $name:ident,
        rows: $rows:expr,
        cols: $cols:expr,
        pio: $pio:ident $(,)?
    ) => {
        $crate::led2d::paste::paste! {
            /// Number of rows in the display.
            $vis const ROWS: usize = $rows;
            /// Number of columns in the display.
            $vis const COLS: usize = $cols;
            /// Total number of LEDs (ROWS * COLS).
            $vis const N: usize = ROWS * COLS;

            const MAPPING: [u16; N] = $crate::led2d::serpentine_column_major_mapping::<N, ROWS, COLS>();

            /// Static resources for the device.
            $vis struct [<$name:camel Static>] {
                led_strip_simple: $crate::led_strip_simple::LedStripSimpleStatic<N>,
                led2d_static: $crate::led2d::Led2dStatic<N>,
            }

            impl [<$name:camel Static>] {
                /// Create static resources.
                #[must_use]
                $vis const fn new_static() -> Self {
                    Self {
                        led_strip_simple: $crate::led_strip_simple::LedStripSimpleStatic::new_static(),
                        led2d_static: $crate::led2d::Led2dStatic::new_static(),
                    }
                }
            }

            // Generate the task wrapper
            $crate::led2d::led2d_device_task!(
                [<$name _device_loop>],
                $crate::led_strip_simple::LedStripSimple<'static, ::embassy_rp::peripherals::$pio, N>,
                N
            );

            /// Device abstraction for the LED matrix.
            $vis struct [<$name:camel>] {
                led2d: $crate::led2d::Led2d<'static, N>,
            }

            impl [<$name:camel>] {
                /// Create static resources.
                #[must_use]
                $vis const fn new_static() -> [<$name:camel Static>] {
                    [<$name:camel Static>]::new_static()
                }

                /// Create the device, spawning the background task.
                ///
                /// # Parameters
                /// - `static_resources`: Static resources created with `new_static()`
                /// - `pio`: PIO peripheral
                /// - `pin`: GPIO pin for LED data
                /// - `max_current`: Maximum current budget
                /// - `spawner`: Task spawner
                $vis async fn new(
                    static_resources: &'static [<$name:camel Static>],
                    pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
                    pin: ::embassy_rp::Peri<'static, impl ::embassy_rp::pio::PioPin>,
                    max_current: $crate::led_strip_simple::Milliamps,
                    spawner: ::embassy_executor::Spawner,
                ) -> $crate::Result<Self> {
                    let strip = $crate::led_strip_simple::LedStripSimple::[<new_ $pio:lower>](
                        &static_resources.led_strip_simple,
                        pio,
                        pin,
                        max_current,
                    )
                    .await;

                    let token = [<$name _device_loop>](
                        &static_resources.led2d_static.command_signal,
                        &static_resources.led2d_static.completion_signal,
                        strip,
                    )?;
                    spawner.spawn(token);

                    let led2d = $crate::led2d::Led2d::new(
                        &static_resources.led2d_static,
                        &MAPPING,
                        COLS,
                    );

                    Ok(Self { led2d })
                }

                /// Render a fully defined frame to the display.
                $vis async fn write_frame(&self, frame: [::smart_leds::RGB8; N]) -> $crate::Result<()> {
                    self.led2d.write_frame(frame).await
                }

                /// Loop through a sequence of animation frames.
                $vis async fn animate(&self, frames: &[$crate::led2d::Frame<N>]) -> $crate::Result<()> {
                    self.led2d.animate(frames).await
                }

                /// Convert (column, row) coordinates to LED strip index.
                #[must_use]
                $vis fn xy_to_index(&self, column_index: usize, row_index: usize) -> usize {
                    self.led2d.xy_to_index(column_index, row_index)
                }
            }

            // Re-export common items for convenience
            $vis use $crate::led_strip_simple::Milliamps;
            $vis use ::smart_leds::colors;
        }
    };
}

pub use led2d_device_simple;
