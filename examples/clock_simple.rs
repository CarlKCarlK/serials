//! Minimal clock example: set an initial UTC time, apply a PDT offset, and log ticks.
#![no_std]
#![no_main]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use panic_probe as _;
use serials::UnixSeconds;
use serials::clock::{Clock, ClockStatic, ONE_SECOND, h12_m_s};

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = run(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn run(spawner: Spawner) -> serials::Result<!> {
    // Initialize RP2040 peripherals to start the time driver.
    let _p = embassy_rp::init(Default::default());

    static CLOCK_STATIC: ClockStatic = Clock::new_static();
    // PDT offset: UTC-7 hours (-420 minutes). Tick every second.
    let clock = Clock::new(&CLOCK_STATIC, -420, Some(ONE_SECOND), spawner);

    // Set current UTC time to 2025-11-20 14:00:00 UTC
    let current_utc_time = UnixSeconds(1_763_647_200);
    clock.set_utc_time(current_utc_time).await;

    let (hour12, minute, second) = h12_m_s(&clock.now_local());
    info!("Local Time: {:02}:{:02}:{:02} PDT", hour12, minute, second);

    clock.set_offset_minutes(-480).await; // Set offset for PST (UTC-8 hours)
    let (hour12, minute, second) = h12_m_s(&clock.now_local());
    info!("Local Time: {:02}:{:02}:{:02} PST", hour12, minute, second);

    loop {
        let local_time = clock.wait_for_tick().await;
        let (hour12, minute, second) = h12_m_s(&local_time);
        info!("Tick: {:02}:{:02}:{:02}", hour12, minute, second);
    }
}
