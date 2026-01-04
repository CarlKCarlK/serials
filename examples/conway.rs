#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_layout::LedLayout;
use device_kit::led_strip::Current;
use device_kit::led_strip::gamma::Gamma;
use device_kit::led_strips;
use device_kit::led2d;
use embassy_executor::Spawner;
use embassy_rp::init;
use embassy_time::{Duration, Timer};
use panic_probe as _;
use smart_leds::colors;

// Two 12x4 panels stacked vertically and rotated 90° CW → 8×12 display.
const LED_LAYOUT_12X4: LedLayout<48, 12, 4> = LedLayout::serpentine_column_major();
const LED_LAYOUT_8X12: LedLayout<96, 8, 12> = LED_LAYOUT_12X4.concat_v(LED_LAYOUT_12X4).rotate_cw();

led2d! {
    pub led8x12,
    pio: PIO0,
    pin: PIN_4,
    dma: DMA_CH0,
    width: 8,
    height: 12,
    led_layout: LED_LAYOUT_8X12,
    max_current: Current::Milliamps(1000),
    gamma: Gamma::Linear,
    max_frames: 32,
    font: Font4x6Trim,
}

const WIDTH: usize = 8;
const HEIGHT: usize = 12;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Conway's Game of Life on 8x12 LED board");
    let p = init(Default::default());

    let led8x12 = Led8x12::new(p.PIO0, p.DMA_CH0, p.PIN_4, spawner)?;

    // Initialize board with a glider pattern centered near the middle
    let mut board = [[false; WIDTH]; HEIGHT];
    // Glider pattern:
    // .X.
    // ..X
    // XXX
    let start_row = 4;
    let start_col = 2;
    board[start_row + 0][start_col + 1] = true;
    board[start_row + 1][start_col + 2] = true;
    board[start_row + 2][start_col + 0] = true;
    board[start_row + 2][start_col + 1] = true;
    board[start_row + 2][start_col + 2] = true;

    loop {
        // Display current board state
        let mut frame = Led8x12::new_frame();
        for row_index in 0..HEIGHT {
            for col_index in 0..WIDTH {
                if board[row_index][col_index] {
                    frame[row_index][col_index] = colors::GREEN;
                }
            }
        }
        led8x12.write_frame(frame).await?;

        // Pause before computing next generation
        Timer::after(Duration::from_millis(50)).await;

        // Compute next generation
        let mut next_board = [[false; WIDTH]; HEIGHT];
        for row_index in 0..HEIGHT {
            for col_index in 0..WIDTH {
                let live_neighbors = count_live_neighbors(&board, row_index, col_index);
                let is_alive = board[row_index][col_index];

                // Conway's Game of Life rules:
                // 1. Any live cell with 2 or 3 live neighbors survives
                // 2. Any dead cell with exactly 3 live neighbors becomes alive
                // 3. All other cells die or stay dead
                next_board[row_index][col_index] = match (is_alive, live_neighbors) {
                    (true, 2) | (true, 3) => true,
                    (false, 3) => true,
                    _ => false,
                };
            }
        }

        board = next_board;
    }
}

/// Count the number of live neighbors for a cell at (row, col).
/// Wraps around board edges (toroidal topology).
fn count_live_neighbors(board: &[[bool; WIDTH]; HEIGHT], row: usize, col: usize) -> u8 {
    let mut count = 0u8;

    // Check all 8 neighbors with wrapping
    for row_offset in [-1, 0, 1].iter().copied() {
        for col_offset in [-1, 0, 1].iter().copied() {
            // Skip the center cell
            if row_offset == 0 && col_offset == 0 {
                continue;
            }

            // Wrap coordinates around board edges
            let neighbor_row = ((row as isize + row_offset).rem_euclid(HEIGHT as isize)) as usize;
            let neighbor_col = ((col as isize + col_offset).rem_euclid(WIDTH as isize)) as usize;

            if board[neighbor_row][neighbor_col] {
                count += 1;
            }
        }
    }

    count
}
