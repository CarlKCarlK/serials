#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

const SPEED_UP_FRACTION: f32 = 1.0; // Speed-up factor: 1.0 = 125 MHz (default), 2.0 = 250 MHz
const HEAP_SIZE: usize = 1024 * 350; // in bytes
const TIME_LIMIT: rp2040_hal::fugit::Duration<u64, 1, 1_000_000> =
    rp2040_hal::fugit::Duration::<u64, 1, 1_000_000>::from_ticks(1_000_000);

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

use rp2040_hal::clocks::ClocksManager;
use rp2040_hal::fugit::RateExtU32;
use rp2040_hal::pll::{setup_pll_blocking, PLLConfig};
use rp2040_hal::xosc::setup_xosc_blocking;
use rp2040_hal::{clocks::ClockSource, pac};

use alloc_cortex_m::CortexMHeap;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use lib::{Never, Result, ONE_DAY};
use malachite::num::arithmetic::traits::CeilingLogBase2;
use malachite::num::arithmetic::traits::SquareAssign;
use malachite::Natural;
// This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

#[expect(clippy::arithmetic_side_effects, reason = "TODO")]
#[expect(unsafe_code, reason = "TODO")]
#[expect(clippy::cast_precision_loss, reason = "TODO")]
#[expect(clippy::assertions_on_constants, reason = "TODO")]
#[expect(clippy::too_many_lines, reason = "TODO")]
#[expect(clippy::cast_sign_loss, reason = "TODO")]
#[expect(clippy::cast_possible_truncation, reason = "TODO")]
async fn inner_main(_spawner: Spawner) -> Result<Never> {
    unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }

    assert!(
        1.0 <= SPEED_UP_FRACTION && SPEED_UP_FRACTION <= 2.0,
        "This is the range I've tested"
    );

    let peripherals = pac::Peripherals::take().expect("Failed to take peripherals");
    // TODO??? let mut _watchdog = Watchdog::new(peripherals.WATCHDOG);

    // Setup the external crystal oscillator (XOSC)
    let xosc_crystal_freq = 12_000_000u32.Hz(); // 12 MHz crystal
    let xosc =
        setup_xosc_blocking(peripherals.XOSC, xosc_crystal_freq).expect("Failed to set up XOSC");

    // Create a ClocksManager instance
    let mut clocks = ClocksManager::new(peripherals.CLOCKS);

    // Dynamically compute the target system clock frequency
    let default_sys_freq = 125_000_000u32; // Default 125 MHz
    let target_sys_freq = (default_sys_freq as f32 * SPEED_UP_FRACTION) as u32; // Target frequency

    // Calculate the VCO frequency and post-dividers
    let post_div1 = 6; // Keep post_div1 constant at 6 for simplicity
    let post_div2 = 1; // Final division step
    let vco_freq = target_sys_freq * post_div1; // VCO frequency

    // Log the calculated values for verification
    info!(
        "Target system frequency: {} Hz, VCO frequency: {} Hz, Post Div1: {}, Post Div2: {}",
        target_sys_freq, vco_freq, post_div1, post_div2
    );

    // Configure PLL_SYS
    let pll_sys_config = PLLConfig {
        refdiv: 1, // Reference divider
        vco_freq: vco_freq.Hz(),
        post_div1: post_div1.try_into().expect("TODO"),
        post_div2,
    };

    let mut resets = peripherals.RESETS;

    let pll_sys = setup_pll_blocking(
        peripherals.PLL_SYS,
        xosc_crystal_freq,
        pll_sys_config,
        &mut clocks,
        &mut resets,
    )
    .expect("Failed to set up PLL_SYS");

    // Configure PLL_USB for compatibility (remains fixed at 48 MHz)
    let pll_usb_config = PLLConfig {
        refdiv: 1,
        vco_freq: 960_000_000u32.Hz(), // VCO frequency: 960 MHz
        post_div1: 5,                  // Divide by 5
        post_div2: 2,                  // Divide by 2 -> 48 MHz
    };

    let pll_usb = setup_pll_blocking(
        peripherals.PLL_USB,
        xosc_crystal_freq,
        pll_usb_config,
        &mut clocks,
        &mut resets,
    )
    .expect("Failed to set up PLL_USB");

    // Initialize the clocks
    clocks
        .init_default(&xosc, &pll_sys, &pll_usb)
        .expect("Failed to initialize clocks");

    info!(
        "System clock frequency: {} Hz",
        clocks.system_clock.get_freq().to_Hz()
    );

    // Use the recalibrated system clock for timer
    let timer = rp2040_hal::Timer::new(peripherals.TIMER, &mut resets, &clocks);

    // // Use the delay function to check system responsiveness
    // let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.get_freq().to_Hz());

    let start = timer.get_counter();
    let mut low = 0;
    let mut high = 1;

    // Exponential search to find an upper bound
    while timer.get_counter() - start < TIME_LIMIT {
        let loop_start = timer.get_counter();
        let result = fibonacci(high);
        let elapsed = timer.get_counter() - loop_start;
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} s)",
            high,
            result.ceiling_log_base_2(),
            elapsed.ticks() as f64 / 1_000_000.0
        );
        if elapsed >= TIME_LIMIT {
            break;
        }
        high *= 2;
    }

    // Binary search to find the largest Fibonacci number that can be generated TIME_LIMIT
    while low < high {
        #[expect(clippy::integer_division_remainder_used, reason = "cmk")]
        let mid = (low + high) / 2;
        let mid_start = timer.get_counter();
        let result = fibonacci(mid);
        let elapsed = timer.get_counter() - mid_start;
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} s)",
            mid,
            result.ceiling_log_base_2(),
            elapsed.ticks() as f64 / 1_000_000.0
        );
        if elapsed < TIME_LIMIT {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    info!(
        "Largest Fibonacci number index that can be generated in less than {}: {}",
        TIME_LIMIT.ticks() as f64 / 1_000_000.0,
        (low - 1)
    );

    // sleep forever
    loop {
        Timer::after(ONE_DAY).await;
    }
}

fn fibonacci(n: usize) -> Natural {
    // fib_fast(n).0 // fib_fast(n-1).1
    fib_two_step(n)
}

#[expect(dead_code, reason = "TODO")]
#[expect(clippy::min_ident_chars, reason = "cmk")]
#[expect(clippy::arithmetic_side_effects, reason = "TODO")]
#[expect(clippy::integer_division_remainder_used, reason = "cmk")]
fn fib_two_step(n: usize) -> Natural {
    if n == 0 {
        return Natural::from(0usize);
    }
    let mut a = Natural::from(0usize);
    let mut b = Natural::from(1usize);
    for _ in 0..((n - 1) / 2) {
        a += &b;
        b += &a;
    }

    if is_even(n) {
        a + b
    } else {
        b
    }
}

#[inline]
const fn is_even(n: usize) -> bool {
    n & 1 == 0
}

const TWO: Natural = Natural::const_from(2);

#[expect(clippy::many_single_char_names, reason = "TODO")]
#[expect(clippy::min_ident_chars, reason = "cmk")]
#[expect(clippy::arithmetic_side_effects, reason = "TODO")]
#[expect(clippy::integer_division_remainder_used, reason = "cmk")]
#[must_use]
pub fn fib_fast(n: usize) -> (Natural, Natural) {
    if n == 0 {
        return (Natural::from(0usize), Natural::from(1usize));
    }

    let (a, mut b) = fib_fast(n / 2);
    let mut c = b.clone();
    c *= TWO;
    c -= &a;
    c *= &a;

    let mut d = a;
    d.square_assign();
    b.square_assign();
    d += &b;

    // let d = &a * &a + &b * &b;
    if n % 2 == 0 {
        (c, d)
    } else {
        c += &d;
        (d, c)
    }
}
