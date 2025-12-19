#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::init;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;
use serials::button::{Button, PressedTo};
use serials::led_strip_simple::Milliamps;
use serials::led2d::led2d_device_simple;
use serials::led12x4::text_frame;
use serials::{Error, Result};
use smart_leds::colors;

// Create the led2d device using the macro
led2d_device_simple! {
    pub led4x12,
    rows: 4,
    cols: 12,
    pio: PIO1,
    mapping: serpentine_column_major,
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("LED 2D API Exploration (12x4 display)");
    let p = init(Default::default());

    static LED4X12_STATIC: Led4x12Static = Led4x12::new_static();
    let led4x12 = Led4x12::new(&LED4X12_STATIC, p.PIO1, p.PIN_3, Milliamps(500), spawner).await?;

    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    loop {
        info!("Demo 1: 3x4 font (\"RUST\" in four colors)");
        demo_rust_text(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 2: Colored corners");
        demo_colored_corners(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 3: Blink pattern");
        demo_blink_pattern(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 4: Rectangle with diagonals (embedded-graphics)");
        demo_rectangle_diagonals_embedded_graphics(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 5: Bouncing dot (manual frames)");
        demo_bouncing_dot_manual(&led4x12, &mut button).await?;

        info!("Demo 6: Bouncing dot (animation)");
        demo_bouncing_dot_animation(&led4x12).await?;
        button.wait_for_press_duration().await;
    }
}

/// Display "RUST" using the built-in 3x4 font helpers from led12x4.
async fn demo_rust_text(led4x12: &Led4x12) -> Result<()> {
    let frame = text_frame(
        ['R', 'U', 'S', 'T'],
        [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
    );
    led4x12.write_frame(frame).await
}

/// Display colored corners to demonstrate coordinate mapping.
async fn demo_colored_corners(led4x12: &Led4x12) -> Result<()> {
    // Four corners with different colors
    let mut frame = Led4x12::new_frame();
    frame[0][0] = colors::RED; // Top-left
    frame[0][Led4x12::COLS - 1] = colors::GREEN; // Top-right
    frame[Led4x12::ROWS - 1][0] = colors::BLUE; // Bottom-left
    frame[Led4x12::ROWS - 1][Led4x12::COLS - 1] = colors::YELLOW; // Bottom-right

    led4x12.write_frame(frame).await?;
    Ok(())
}

/// Blink a pattern by constructing frames explicitly.
async fn demo_blink_pattern(led4x12: &Led4x12) -> Result<()> {
    // Create checkerboard pattern
    let mut on_frame = Led4x12::new_frame();
    for row_index in 0..Led4x12::ROWS {
        for column_index in 0..Led4x12::COLS {
            if (row_index + column_index) % 2 == 0 {
                on_frame[row_index][column_index] = colors::CYAN;
            }
        }
    }

    led4x12
        .animate(&[
            (on_frame, Duration::from_millis(500)),
            (Led4x12::new_frame(), Duration::from_millis(500)),
        ])
        .await
}

/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics(led4x12: &Led4x12) -> Result<()> {
    use embedded_graphics::{
        Drawable,
        pixelcolor::Rgb888,
        prelude::*,
        primitives::{Line, PrimitiveStyle, Rectangle},
    };
    use serials::led2d::Frame;

    let mut frame = Led4x12::new_frame();

    // Use the embedded_graphics crate to draw an image.

    // Draw red rectangle border
    Rectangle::new(Frame::<4, 12>::top_left(), Frame::<4, 12>::size())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
        .draw(&mut frame)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(Frame::<4, 12>::top_left(), Frame::<4, 12>::bottom_right())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
        .draw(&mut frame)?;

    Line::new(Frame::<4, 12>::bottom_left(), Frame::<4, 12>::top_right())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
        .draw(&mut frame)?;

    led4x12.write_frame(frame).await
}

async fn demo_bouncing_dot_manual(led4x12: &Led4x12, button: &mut Button<'_>) -> Result<()> {
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
    let (x_limit, y_limit) = (Led4x12::COLS as isize, Led4x12::ROWS as isize);
    let mut color = *color_cycle.next().unwrap(); // Safe: cycle() over a non-empty array never returns None

    loop {
        let mut frame = Led4x12::new_frame();
        frame[y as usize][x as usize] = color;
        led4x12.write_frame(frame).await?;

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
async fn demo_bouncing_dot_animation(led4x12: &Led4x12) -> Result<()> {
    use serials::led2d::ANIMATION_MAX_FRAMES;

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

    let mut frames = Vec::<_, ANIMATION_MAX_FRAMES>::new();
    let (mut x, mut y) = (0isize, 0isize);
    let (mut vx, mut vy) = (1isize, 1isize);
    let (x_limit, y_limit) = (Led4x12::COLS as isize, Led4x12::ROWS as isize);
    let mut color = *color_cycle.next().unwrap();

    for _ in 0..ANIMATION_MAX_FRAMES {
        let mut frame = Led4x12::new_frame();
        frame[y as usize][x as usize] = color;
        frames
            .push((frame, Duration::from_millis(50)))
            .map_err(|_| Error::FormatError)?;

        if step_and_hit(&mut x, &mut vx, x_limit) | step_and_hit(&mut y, &mut vy, y_limit) {
            color = *color_cycle.next().unwrap();
        }
    }

    led4x12.animate(&frames).await
}
