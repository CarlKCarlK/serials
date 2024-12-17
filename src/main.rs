//! A 4-digit 7-segment clock that can be controlled by a button.
//!
//! Runs on a Raspberry Pi Pico RP2040. See the `README.md` for more information.
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();
const HEAP_SIZE: usize = 1024 * 64; // in bytes

use alloc_cortex_m::CortexMHeap;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use lib::{Never, Result, ONE_DAY};
use num_bigint::BigUint;
// This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

#[expect(clippy::arithmetic_side_effects, reason = "cmk")]
#[expect(unsafe_code, reason = "cmk")]
async fn inner_main(_spawner: Spawner) -> Result<Never> {
    unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }

    let start = Instant::now();
    let mut low = 0;
    let mut high = 1;

    // Exponential search to find an upper bound
    while Instant::now().duration_since(start) < Duration::from_secs(1) {
        let loop_start = Instant::now();
        let result = fibonacci(high);
        let elapsed = loop_start.elapsed();
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} ms)",
            high,
            result.bits(),
            elapsed.as_millis()
        );
        if elapsed >= Duration::from_secs(1) {
            break;
        }
        high *= 2;
    }

    // Binary search to find the largest Fibonacci number that can be generated in less than 1 second
    while low < high {
        #[expect(clippy::integer_division_remainder_used, reason = "cmk")]
        let mid = (low + high) / 2;
        let mid_start = Instant::now();
        let result = fibonacci(mid);
        let elapsed = mid_start.elapsed();
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} ms)",
            mid,
            result.bits(),
            elapsed.as_millis()
        );
        if elapsed < Duration::from_secs(1) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    info!(
        "Largest Fibonacci number index that can be generated in less than one second: {}",
        (low - 1)
    );

    // sleep forever
    loop {
        Timer::after(ONE_DAY).await;
    }
}

#[expect(clippy::min_ident_chars, reason = "cmk")]
#[expect(clippy::arithmetic_side_effects, reason = "cmk")]
fn fibonacci(n: u64) -> BigUint {
    if n == 0 {
        return BigUint::from(0u64);
    }
    let mut a = BigUint::from(0u64);
    let mut b = BigUint::from(1u64);
    #[expect(clippy::integer_division_remainder_used, reason = "cmk")]
    for _ in 0..((n - 1) / 2) {
        a += &b;
        b += &a;
    }

    if n & 1 == 0 {
        // n is even
        a + b
    } else {
        b
    }
}
