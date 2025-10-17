#![no_std]
#![no_main]

//! IR Receiver NEC Protocol Decoder
//!
//! Microsecond-precision edge-based approach.
//! Polls GPIO6 for state changes and timestamps them with microsecond accuracy.
//!
//! Run with: `cargo run --example ir_pio_read` or `cargo ir`

use defmt::info;
use defmt_rtt as _;
use embassy_rp::gpio::Input;
use embassy_executor::Spawner;
use embassy_time::Instant;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());
    
    // GPIO6 as input for IR sensor
    let ir_pin = Input::new(p.PIN_6, embassy_rp::gpio::Pull::None);

    info!("IR NEC Decoder - Microsecond-Precision Timing");
    info!("IR pin initialized on GPIO 6");
    info!("Waiting for IR signals...");

    let mut edge_times: heapless::Vec<u32, 256> = heapless::Vec::new();
    let mut last_timestamp: u32 = 0;
    let mut in_frame = false;
    let mut last_state = ir_pin.is_low();
    let mut event_count = 0u32;

    loop {
        // Sample pin state (roughly every 100µs due to Timer::after)
        let current_state = ir_pin.is_low();
        
        if current_state != last_state {
            // Edge detected!
            let timestamp = Instant::now().as_micros() as u32;
            
            // Calculate delta from last edge
            let delta_us = if timestamp >= last_timestamp {
                timestamp - last_timestamp
            } else {
                // Handle u32 overflow
                u32::MAX - last_timestamp + timestamp
            };
            
            let delta_ms = (delta_us + 500) / 1000; // Round to nearest ms

            // First edge - initialize frame
            if !in_frame && event_count == 0 {
                in_frame = true;
                edge_times.clear();
                info!("Frame start detected");
            }

            if in_frame {
                let _ = edge_times.push(delta_ms);
                event_count += 1;
                last_state = current_state;

                // NEC frame structure:
                // Leader: LOW (~9ms) + HIGH (~4.5ms) = 2 edges
                // Data: 32 bits × 2 edges/bit = 64 edges  
                // Total: ~66 edges for a complete frame
                // Plus we capture the idle gap first, so 67+ edges minimum
                
                // After 67+ edges, we likely have a complete frame
                // If we also see a gap > 40ms, the frame has definitely ended
                if event_count >= 67 && delta_ms > 40 {
                    info!("✓ Frame complete: {} edges (gap {}ms)", edge_times.len(), delta_ms);
                    if let Some((addr, cmd)) = decode_nec(&edge_times) {
                        info!("✓✓ NEC: Addr=0x{:02X} Cmd=0x{:02X}", addr, cmd);
                    }
                    in_frame = false;
                    edge_times.clear();
                    event_count = 0;
                } else if event_count >= 75 {
                    // If we have way more edges than expected, something is wrong
                    // (maybe multiple frames got merged), so force a decode anyway
                    info!("Frame has {} edges (over 75) - forcing decode", edge_times.len());
                    if let Some((addr, cmd)) = decode_nec(&edge_times) {
                        info!("✓✓ NEC: Addr=0x{:02X} Cmd=0x{:02X}", addr, cmd);
                    }
                    in_frame = false;
                    edge_times.clear();
                    event_count = 0;
                }
            }

            last_timestamp = timestamp;
        } else {
            // No edge - check for timeout
            if in_frame && event_count > 0 {
                let now = Instant::now().as_micros() as u32;
                let silence_us = if now >= last_timestamp {
                    now - last_timestamp
                } else {
                    u32::MAX - last_timestamp + now
                };
                let silence_ms = (silence_us + 500) / 1000;

                // If > 200ms silence with reasonable frame, decode it
                if silence_ms > 200 && edge_times.len() >= 50 {
                    info!("Frame timeout ({}ms silence) - decoding {} edges", silence_ms, edge_times.len());
                    if let Some((addr, cmd)) = decode_nec(&edge_times) {
                        info!("✓✓ NEC: Addr=0x{:02X} Cmd=0x{:02X}", addr, cmd);
                    }
                    in_frame = false;
                    edge_times.clear();
                    event_count = 0;
                }
            }
        }

        // Yield briefly (100µs) to let other tasks run
        embassy_time::Timer::after(embassy_time::Duration::from_micros(100)).await;
    }
}

/// Decode NEC protocol from edge timing deltas (in milliseconds)
fn decode_nec(timings: &heapless::Vec<u32, 256>) -> Option<(u8, u8)> {
    if timings.len() < 4 {
        info!("  Insufficient timings: {}", timings.len());
        return None;
    }

    // NEC frame structure:
    // timings[0] = idle-to-frame gap (very large, skip it)
    // timings[1+] = actual NEC frame starting with leader
    //   leader LOW (~9ms) + leader HIGH (~4.5ms) + 32 bits + final LOW
    //
    // Search for the leader pattern (9ms LOW, 4.5ms HIGH) which is distinctive
    let mut leader_index = None;
    
    for i in 0..timings.len().saturating_sub(1) {
        let low = timings[i];
        let high = timings[i + 1];
        
        // Look for: LOW in range 8-10ms, HIGH in range 4-5ms
        if low >= 8 && low <= 10 && high >= 4 && high <= 5 {
            leader_index = Some(i);
            break;
        }
    }

    let frame_start = match leader_index {
        Some(idx) => idx,
        None => {
            info!("  No leader pattern found in {} timings", timings.len());
            return None;
        }
    };

    let leader_low = timings[frame_start];
    let leader_high = timings[frame_start + 1];

    info!("  Leader found at index {}: LOW={}ms HIGH={}ms", frame_start, leader_low, leader_high);

    // NEC leader validation
    // Standard: 9ms LOW ± 0.5ms, 4.5ms HIGH ± 0.5ms
    // Our search already narrowed it, so this is just a sanity check
    if leader_low < 8 || leader_low > 10 {
        info!("  ✗ Leader LOW out of range ({}ms)", leader_low);
        return None;
    }
    if leader_high < 4 || leader_high > 5 {
        info!("  ✗ Leader HIGH out of range ({}ms)", leader_high);
        return None;
    }

    // Decode 32 bits from timing pairs after the leader
    let mut bits = heapless::Vec::<u8, 32>::new();
    let mut i = frame_start + 2; // Skip leader_low and leader_high

    while bits.len() < 32 && i + 1 < timings.len() {
        let _low = timings[i];
        let high = timings[i + 1];

        // NEC bit encoding (HIGH pulse duration discriminates):
        // 0 bit: 560µs LOW + 560µs HIGH   (~1ms total in 1ms bins -> HIGH~0ms)
        // 1 bit: 560µs LOW + 1.69ms HIGH  (~2.2ms total in 1ms bins -> HIGH~2ms)
        // Threshold: HIGH >= 1 is a "1" bit
        let bit = if high >= 1 { 1u8 } else { 0u8 };
        let _ = bits.push(bit);

        i += 2;
    }

    info!("  Extracted {} bits", bits.len());

    // Validate frame has full 32 bits
    if bits.len() < 32 {
        info!("  ✗ Only {} bits extracted (need 32)", bits.len());
        return None;
    }

    // Extract address (bits 0-7) and command (bits 16-23)
    // NEC uses LSB-first transmission within each byte
    let mut addr = 0u8;
    let mut cmd = 0u8;

    for j in 0..8 {
        addr |= bits[j] << j;
        cmd |= bits[16 + j] << j;
    }

    Some((addr, cmd))
}
