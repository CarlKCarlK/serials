//! A device abstraction for infrared receivers using the NEC protocol.
//!
//! See [`Ir`] for usage examples.
// nec_ir.rs
use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::Peri;
use embassy_rp::gpio::{AnyPin, Input, Pin, Pull};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use embassy_time::Instant;

use crate::{Error, Result};

// ===== Public API ===========================================================

/// Events received from the infrared receiver.
///
/// See [`Ir`] for usage examples.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum IrEvent {
    /// Button press with 16-bit address and 8-bit command.
    /// Supports both standard NEC (8-bit address) and extended NEC (16-bit address).
    Press { addr: u16, cmd: u8 },
    // Repeat { addr: u16, cmd: u8 },
}

/// Static type for the `Ir` device abstraction.
///
/// See [`Ir`] for usage examples.
pub type IrStatic = EmbassyChannel<CriticalSectionRawMutex, IrEvent, 8>;

/// A device abstraction for an infrared receiver using the NEC protocol.
///
/// # Examples
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// # use defmt::info;
/// # use embassy_executor::Spawner;
/// # use serials::ir::{Ir, IrEvent};
/// # async fn example(p: embassy_rp::Peripherals, spawner: Spawner) -> serials::Result<()> {
/// static IR_STATIC: serials::ir::IrStatic = Ir::new_static();
/// let ir = Ir::new(p.PIN_15, &IR_STATIC, spawner)?;
///
/// loop {
///     let event = ir.wait().await;
///     match event {
///         IrEvent::Press { addr, cmd } => {
///             info!("IR: addr=0x{:04X}, cmd=0x{:02X}", addr, cmd);
///         }
///     }
/// }
/// # }
/// ```
pub struct Ir<'a> {
    ir_static: &'a IrStatic,
}

impl Ir<'_> {
    /// Create static channel resources for IR events.
    ///
    /// See [`Ir`] for usage examples.
    #[must_use]
    pub const fn new_static() -> IrStatic {
        EmbassyChannel::new()
    }

    /// Create a new IR receiver on the specified pin.
    ///
    /// See [`Ir`] for usage examples.
    ///
    /// # Errors
    /// Returns an error if the background task cannot be spawned.
    pub fn new<P: Pin>(
        pin: Peri<'static, P>,
        ir_static: &'static IrStatic,
        spawner: Spawner,
    ) -> Result<Self> {
        // Type erase to Peri<'static, AnyPin> (keep the Peri wrapper!)
        let any: Peri<'static, AnyPin> = pin.into();
        // Use Pull::Up for typical IR receivers (they idle HIGH with active-low modules)
        let token = nec_ir_task(Input::new(any, Pull::Up), ir_static).map_err(Error::TaskSpawn)?;
        spawner.spawn(token);
        Ok(Self { ir_static })
    }

    /// Wait for the next IR event.
    ///
    /// See [`Ir`] for usage examples.
    pub async fn wait(&self) -> IrEvent {
        self.ir_static.receive().await
    }
}

#[embassy_executor::task]
async fn nec_ir_task(mut pin: Input<'static>, ir_static: &'static IrStatic) -> ! {
    let mut decoder_state: DecoderState = DecoderState::Idle;
    let mut last_code: Option<(u16, u8)> = None;
    let mut level_low: bool = pin.is_low(); // Initialize from pin state
    let mut last_edge: Instant = Instant::now();

    info!("NEC IR task started");
    loop {
        pin.wait_for_any_edge().await;

        let now = Instant::now();
        let dt = now.duration_since(last_edge).as_micros() as u32;
        // info!("NEC IR edge: dt={}µs", dt);
        last_edge = now;

        // Active-low receiver: every edge toggles the level.
        // Toggle instead of reading pin to avoid race conditions and glitches
        level_low = !level_low;

        // Sanity check: verify our toggle matches the actual pin state
        let actual_level_low = pin.is_low();
        if level_low != actual_level_low {
            defmt::warn!(
                "IR: Pin state mismatch! Expected {}, got {} (missed edge?)",
                level_low,
                actual_level_low
            );
            // Resync to actual pin state
            level_low = actual_level_low;
            // Reset decoder to avoid processing corrupt data
            decoder_state = DecoderState::Idle;
            continue;
        }

        // info!("NEC IR state: {}", decoder_state.name());

        let (decoder_state0, ir_nec_event, last_code0) =
            feed(decoder_state, level_low, dt, last_code);
        decoder_state = decoder_state0;
        last_code = last_code0;

        if let Some(ir_event) = ir_nec_event {
            ir_static.send(ir_event).await;
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum DecoderState {
    Idle,
    LdrLow,
    LdrHigh,
    BitLow { n: u8, v: u32 },
    BitHigh { n: u8, v: u32 },
    StopBit { addr: u16, cmd: u8 }, // Waiting for final stop bit after 32 bits
    RepeatTail,
}

// impl DecoderState {
//     fn name(&self) -> &'static str {
//         match self {
//             DecoderState::Idle => "Idle",
//             DecoderState::LdrLow => "LdrLow",
//             DecoderState::LdrHigh => "LdrHigh",
//             DecoderState::BitLow { .. } => "BitLow",
//             DecoderState::BitHigh { .. } => "BitHigh",
//             DecoderState::StopBit { .. } => "StopBit",
//             DecoderState::RepeatTail => "RepeatTail",
//         }
//     }
// }

#[inline]
fn inr(x: u32, r: (u32, u32)) -> bool {
    x >= r.0 && x <= r.1
}
#[inline]
fn nec_ok(f: u32) -> Option<(u16, u8)> {
    let b0 = (f & 0xFF) as u8;
    let b1 = ((f >> 8) & 0xFF) as u8;
    let b2 = ((f >> 16) & 0xFF) as u8;
    let b3 = ((f >> 24) & 0xFF) as u8;

    // Validate command with its inverse (required in both variants)
    if (b2 ^ b3) != 0xFF {
        return None;
    }

    // Standard NEC: second byte is inverse of the first (8-bit address)
    if (b0 ^ b1) == 0xFF {
        return Some((b0 as u16, b2));
    }

    // Extended NEC: two address bytes (16-bit address)
    let addr16 = ((b1 as u16) << 8) | (b0 as u16);
    Some((addr16, b2))
}

// µs windows - RELAXED TOLERANCES for better reliability
const GLITCH: u32 = 120;
const MIN_IDLE: u32 = 5_000; // Require 5ms of idle before starting decode (filters SPI crosstalk)
const LDR_LOW: (u32, u32) = (7_000, 11_000); // was (7_500, 10_500) - ±15%
const LDR_HIGH: (u32, u32) = (3_500, 5_500); // was (3_700, 5_300) - ±22%
const REP_HIGH: (u32, u32) = (1_500, 3_000); // was (1_750, 2_750) - ±33%
const BIT_LOW: (u32, u32) = (300, 900); // was (360, 760) - ±40%
const BIT0_HIGH: (u32, u32) = (250, 900); // was (310, 810) - ±56%
const BIT1_HIGH: (u32, u32) = (1_000, 2_400); // was (1_190, 2_190) - ±40%

// cmk move into an impl
fn feed(
    mut decoder_state: DecoderState,
    level_low: bool,
    dt: u32,
    mut last_code: Option<(u16, u8)>,
) -> (DecoderState, Option<IrEvent>, Option<(u16, u8)>) {
    if dt < GLITCH {
        return (decoder_state, None, last_code);
    }
    use DecoderState::*;
    match decoder_state {
        Idle => {
            // Only start decoding if we've been idle (HIGH) for at least MIN_IDLE
            // This filters out SPI crosstalk and other electrical noise
            if level_low && dt >= MIN_IDLE {
                decoder_state = LdrLow;
                defmt::info!("IR: Decoding started");
            }
        }
        LdrLow => {
            if !level_low && inr(dt, LDR_LOW) {
                decoder_state = LdrHigh;
            } else {
                decoder_state = Idle;
                // Only log decode failures for pulses that were at least somewhat close
                // Very short pulses (<2ms) are likely NEC stop bits, not decode failures
                if dt > 2_000 {
                    defmt::info!("IR: Decode failed (bad LDR_LOW timing)");
                }
            }
        }
        LdrHigh => {
            if level_low && inr(dt, LDR_HIGH) {
                decoder_state = BitLow { n: 0, v: 0 };
            } else if level_low && inr(dt, REP_HIGH) {
                decoder_state = RepeatTail;
            } else {
                decoder_state = Idle;
                defmt::info!("IR: Decode failed (bad LDR_HIGH/REP_HIGH timing)");
            }
        }
        RepeatTail => {
            if !level_low && inr(dt, BIT_LOW) {
                // CMK let out = last_code.map(|(a, c)| IrNecEvent::Repeat { addr: a, cmd: c });
                decoder_state = Idle;
                // cmk return (decoder_state, out, last_code);
            } else {
                decoder_state = Idle;
                defmt::info!("IR: Decode failed (bad RepeatTail timing)");
            }
        }
        BitLow { n, v } => {
            if !level_low && inr(dt, BIT_LOW) {
                decoder_state = BitHigh { n, v };
            } else {
                decoder_state = Idle;
                defmt::info!("IR: Decode failed (bad BIT_LOW timing, bit={})", n);
            }
        }
        BitHigh { n, mut v } => {
            if level_low && inr(dt, BIT1_HIGH) {
                v |= 1u32 << n;
            } else if !(level_low && inr(dt, BIT0_HIGH)) {
                decoder_state = Idle;
                defmt::info!("IR: Decode failed (bad BIT_HIGH timing, bit={})", n);
                return (decoder_state, None, last_code);
            }

            let n2 = n + 1;
            if n2 == 32 {
                if let Some((a, c)) = nec_ok(v) {
                    last_code = Some((a, c));
                    // Don't emit the event yet - wait for stop bit validation
                    decoder_state = StopBit { addr: a, cmd: c };
                } else {
                    decoder_state = Idle;
                    defmt::info!(
                        "IR: Decode failed (checksum validation failed, v=0x{:08X})",
                        v
                    );
                }
            } else {
                decoder_state = BitLow { n: n2, v };
            }
        }
        StopBit { addr, cmd } => {
            // NEC stop bit: short low pulse (~562µs)
            if !level_low && inr(dt, BIT_LOW) {
                decoder_state = Idle;
                // Stop bit validated - emit the event
                return (decoder_state, Some(IrEvent::Press { addr, cmd }), last_code);
            } else {
                decoder_state = Idle;
                defmt::info!("IR: Decode failed (missing or bad stop bit, dt={}µs)", dt);
            }
        }
    }
    (decoder_state, None, last_code)
}
