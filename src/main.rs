//! A 4-digit 7-segment clock that can be controlled by a button.
//!
//! Runs on a Raspberry Pi Pico RP2040. See the `README.md` for more information.
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();
const HEAP_SIZE: usize = 1024 * 64; // in bytes

use rp2040_hal::clocks::{Clock, ClocksManager};
use rp2040_hal::fugit::RateExtU32;
use rp2040_hal::pll::{setup_pll_blocking, PLLConfig};
use rp2040_hal::xosc::setup_xosc_blocking;
use rp2040_hal::{
    clocks::{init_clocks_and_plls, ClockSource},
    pac,
    watchdog::Watchdog,
};

use alloc_cortex_m::CortexMHeap;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
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
#[expect(clippy::cast_precision_loss, reason = "cmk")]
async fn inner_main(_spawner: Spawner) -> Result<Never> {
    unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }

    let mut peripherals = pac::Peripherals::take().unwrap();
    let mut watchdog = Watchdog::new(peripherals.WATCHDOG);

    // Setup the external crystal oscillator (XOSC)
    let xosc_crystal_freq = 12_000_000u32.Hz(); // 12 MHz crystal
    let xosc =
        setup_xosc_blocking(peripherals.XOSC, xosc_crystal_freq).expect("Failed to set up XOSC");

    // Create a ClocksManager instance
    let mut clocks = ClocksManager::new(peripherals.CLOCKS);

    // Configure PLL_SYS to 250 MHz
    let pll_sys_config = PLLConfig {
        refdiv: 1,                       // Reference divider
        vco_freq: 1_500_000_000u32.Hz(), // VCO frequency: 1500 MHz
        post_div1: 6,                    // Divide by 6
        post_div2: 1,                    // Divide by 1 -> 250 MHz
    };

    let mut resets = peripherals.RESETS;

    let pll_sys = setup_pll_blocking(
        peripherals.PLL_SYS,
        xosc_crystal_freq,
        pll_sys_config,
        &mut clocks, // Pass ClocksManager
        &mut resets,
    )
    .expect("Failed to set up PLL_SYS");

    // Configure PLL_USB for compatibility
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
        &mut clocks, // Pass ClocksManager
        &mut resets,
    )
    .expect("Failed to set up PLL_USB");

    // Initialize the clocks
    clocks.init_default(&xosc, &pll_sys, &pll_usb);

    info!(
        "System clock frequency: {} Hz",
        clocks.system_clock.get_freq().to_Hz()
    ); // Verify the system clock frequency

    // Use the recalibrated system clock for timer
    let timer = rp2040_hal::Timer::new(peripherals.TIMER, &mut resets, &clocks);

    let one_second = rp2040_hal::fugit::Duration::<u64, 1, 1_000_000>::from_ticks(1_000_000);

    // // Use the delay function to check system responsiveness
    // let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.get_freq().to_Hz());

    let start = timer.get_counter();
    let mut low = 0;
    let mut high = 1;

    // Exponential search to find an upper bound
    while timer.get_counter() - start < one_second {
        let loop_start = timer.get_counter();
        let result = fibonacci(high);
        let elapsed = timer.get_counter() - loop_start;
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} s)",
            high,
            result.bits(),
            elapsed.ticks() as f64 / 1_000_000.0
        );
        if elapsed >= one_second {
            break;
        }
        high *= 2;
    }

    // Binary search to find the largest Fibonacci number that can be generated in less than 1 second
    while low < high {
        #[expect(clippy::integer_division_remainder_used, reason = "cmk")]
        let mid = (low + high) / 2;
        let mid_start = timer.get_counter();
        let result = fibonacci(mid);
        let elapsed = timer.get_counter() - mid_start;
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} s)",
            mid,
            result.bits(),
            elapsed.ticks() as f64 / 1_000_000.0
        );
        if elapsed < one_second {
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
