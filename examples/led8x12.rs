#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::button::{Button, PressedTo};
use device_kit::led_strip_simple::Milliamps;
use device_kit::led2d;
use device_kit::{Error, Result};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::init;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;
use smart_leds::colors;

// Rotated display: 8 wide × 12 tall (two 12x4 panels rotated 90° clockwise)
// Better for clock display - can fit 2 lines of 2 digits each
led2d! {
    pub led8x12,
    pio: PIO0,
    pin: PIN_4,
    dma: DMA_CH0,
    rows: 12,
    cols: 8,
    mapping: arbitrary([
        47, 46, 45, 44, 95, 94, 93, 92,
        40, 41, 42, 43, 88, 89, 90, 91,
        39, 38, 37, 36, 87, 86, 85, 84,
        32, 33, 34, 35, 80, 81, 82, 83,
        31, 30, 29, 28, 79, 78, 77, 76,
        24, 25, 26, 27, 72, 73, 74, 75,
        23, 22, 21, 20, 71, 70, 69, 68,
        16, 17, 18, 19, 64, 65, 66, 67,
        15, 14, 13, 12, 63, 62, 61, 60,
        8, 9, 10, 11, 56, 57, 58, 59,
        7, 6, 5, 4, 55, 54, 53, 52,
        0, 1, 2, 3, 48, 49, 50, 51,
    ]),
    max_current: Milliamps(1000),
    max_frames: 32,
    font: Font4x6Trim,
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("LED 2D API Exploration (8x12 rotated display)");
    let p = init(Default::default());

    let led8x12 = Led8x12::new(p.PIO0, p.DMA_CH0, p.PIN_4, spawner)?;

    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    loop {
        info!("Demo 1: Clock-style two-line text");
        demo_clock_text(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 2: Colored corners (orientation test)");
        demo_colored_corners(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 3: Blink text");
        demo_blink_text(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 4: Blink pattern");
        demo_blink_pattern(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 5: Rectangle with diagonals (embedded-graphics)");
        demo_rectangle_diagonals_embedded_graphics(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 6: Bouncing dot (manual frames)");
        demo_bouncing_dot_manual(&led8x12, &mut button).await?;

        info!("Demo 7: Bouncing dot (animation)");
        demo_bouncing_dot_animation(&led8x12).await?;
        button.wait_for_press_duration().await;
    }
}

/// Display time-like text using two lines (like "12" on top, "34" on bottom).
async fn demo_clock_text(led8x12: &Led8x12) -> Result<()> {
    let colors = [colors::CYAN, colors::MAGENTA, colors::ORANGE, colors::LIME];
    led8x12.write_text("12\n34", &colors).await
}

/// Blink text by constructing frames explicitly.
async fn demo_blink_text(led8x12: &Led8x12) -> Result<()> {
    let mut on_frame = Led8x12::new_frame();
    led8x12.write_text_to_frame("HI", &[colors::YELLOW], &mut on_frame)?;
    led8x12
        .animate(&[
            (on_frame, Duration::from_millis(500)),
            (Led8x12::new_frame(), Duration::from_millis(500)),
        ])
        .await
}

/// Display colored corners to demonstrate coordinate mapping.
async fn demo_colored_corners(led8x12: &Led8x12) -> Result<()> {
    // Four corners with different colors
    let mut frame = Led8x12::new_frame();
    frame[0][0] = colors::RED; // Top-left
    frame[0][Led8x12::COLS - 1] = colors::GREEN; // Top-right
    frame[Led8x12::ROWS - 1][0] = colors::BLUE; // Bottom-left
    frame[Led8x12::ROWS - 1][Led8x12::COLS - 1] = colors::YELLOW; // Bottom-right

    led8x12.write_frame(frame).await?;
    Ok(())
}

/// Blink a pattern by constructing frames explicitly.
async fn demo_blink_pattern(led8x12: &Led8x12) -> Result<()> {
    // Create checkerboard pattern
    let mut on_frame = Led8x12::new_frame();
    for row_index in 0..Led8x12::ROWS {
        for column_index in 0..Led8x12::COLS {
            if (row_index + column_index) % 2 == 0 {
                on_frame[row_index][column_index] = colors::PURPLE;
            }
        }
    }

    led8x12
        .animate(&[
            (on_frame, Duration::from_millis(500)),
            (Led8x12::new_frame(), Duration::from_millis(500)),
        ])
        .await
}

/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics(led8x12: &Led8x12) -> Result<()> {
    use device_kit::led2d::Frame;
    use embedded_graphics::{
        Drawable,
        pixelcolor::Rgb888,
        prelude::*,
        primitives::{Line, PrimitiveStyle, Rectangle},
    };

    let mut frame = Led8x12::new_frame();

    // Use the embedded_graphics crate to draw an image.

    // Draw red rectangle border
    Rectangle::new(Frame::<12, 8>::top_left(), Frame::<12, 8>::size())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
        .draw(&mut frame)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(Frame::<12, 8>::top_left(), Frame::<12, 8>::bottom_right())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
        .draw(&mut frame)?;

    Line::new(Frame::<12, 8>::bottom_left(), Frame::<12, 8>::top_right())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
        .draw(&mut frame)?;

    led8x12.write_frame(frame).await
}

async fn demo_bouncing_dot_manual(led8x12: &Led8x12, button: &mut Button<'_>) -> Result<()> {
    let mut color_cycle = [colors::RED, colors::GREEN, colors::BLUE].iter().cycle();

    // Steps one position coordinate and reports if it hit an edge.
    fn step_and_hit(position: &mut isize, velocity: &mut isize, limit: isize) -> bool {
        *position += *velocity;
        if (0..limit).contains(position) {
            return false;
        }
        *velocity = -*velocity;
        *position += *velocity; // step back inside
        true
    }

    let (mut x, mut y) = (0isize, 0isize);
    let (mut vx, mut vy) = (1isize, 1isize);
    let (x_limit, y_limit) = (Led8x12::COLS as isize, Led8x12::ROWS as isize);
    let mut color = *color_cycle.next().unwrap(); // Safe: cycle() over a non-empty array never returns None

    loop {
        let mut frame = Led8x12::new_frame();
        frame[y as usize][x as usize] = color;
        led8x12.write_frame(frame).await?;

        if step_and_hit(&mut x, &mut vx, x_limit) | step_and_hit(&mut y, &mut vy, y_limit) {
            color = *color_cycle.next().unwrap();
        }

        if let Either::Second(_) = select(Timer::after_millis(50), button.wait_for_press()).await {
            break;
        }
    }

    Ok(())
}

/// Bouncing dot using pre-built animation frames.
async fn demo_bouncing_dot_animation(led8x12: &Led8x12) -> Result<()> {
    let mut color_cycle = [colors::CYAN, colors::YELLOW, colors::LIME].iter().cycle();

    // Steps one position coordinate and reports if it hit an edge.
    fn step_and_hit(position: &mut isize, velocity: &mut isize, limit: isize) -> bool {
        *position += *velocity;
        if (0..limit).contains(position) {
            return false;
        }
        *velocity = -*velocity;
        *position += *velocity; // step back inside
        true
    }

    let mut frames = Vec::<_, { Led8x12::MAX_FRAMES }>::new();
    let (mut x, mut y) = (0isize, 0isize);
    let (mut vx, mut vy) = (1isize, 1isize);
    let (x_limit, y_limit) = (Led8x12::COLS as isize, Led8x12::ROWS as isize);
    let mut color = *color_cycle.next().unwrap();

    for _ in 0..Led8x12::MAX_FRAMES {
        let mut frame = Led8x12::new_frame();
        frame[y as usize][x as usize] = color;
        frames
            .push((frame, Duration::from_millis(50)))
            .map_err(|_| Error::FormatError)?;

        if step_and_hit(&mut x, &mut vx, x_limit) | step_and_hit(&mut y, &mut vy, y_limit) {
            color = *color_cycle.next().unwrap();
        }
    }

    led8x12.animate(&frames).await
}
