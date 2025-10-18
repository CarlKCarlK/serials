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

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum IrNecEvent {
    Press { addr: u8, cmd: u8 },
    Repeat { addr: u8, cmd: u8 },
}

pub type IrNecNotifier = EmbassyChannel<CriticalSectionRawMutex, IrNecEvent, 8>;

pub struct IrNec<'a> {
    notifier: &'a IrNecNotifier,
}

impl IrNec<'_> {
    #[must_use]
    pub const fn notifier() -> IrNecNotifier {
        EmbassyChannel::new()
    }

    pub fn new<P: Pin>(
        pin: Peri<'static, P>,
        pull: Pull,
        notifier: &'static IrNecNotifier,
        spawner: Spawner,
    ) -> Result<Self> {
        // Type erase to Peri<'static, AnyPin> (keep the Peri wrapper!)
        let any: Peri<'static, AnyPin> = pin.into();
        spawner
            .spawn(nec_ir_task(NecIrDevice::new(any, pull), notifier))
            .map_err(Error::TaskSpawn)?;
        Ok(Self { notifier })
    }

    pub async fn next_event(&self) -> IrNecEvent {
        self.notifier.receive().await
    }
}

// ===== Concrete device passed to the task (non-generic) =====================

struct NecIrDevice {
    pin: Input<'static>, // NOTE: Input<'d> has NO pin type param in embassy-rp 0.8
                         // dec: Decoder,
}

impl NecIrDevice {
    fn new(pin: Peri<'static, AnyPin>, pull: Pull) -> Self {
        let pin = Input::new(pin, pull);
        Self { pin }
    }
}

// ===== The non-generic task =================================================

#[embassy_executor::task]
async fn nec_ir_task(mut nec_ir_device: NecIrDevice, notifier: &'static IrNecNotifier) -> ! {
    let mut decoder_state: DecoderState = DecoderState::Idle;
    let mut last_code: Option<(u8, u8)> = None;
    let mut level_low: bool = nec_ir_device.pin.is_low();
    let mut last_edge: Instant = Instant::now();

    info!("NEC IR task started");
    loop {
        nec_ir_device.pin.wait_for_any_edge().await;

        let now = Instant::now();
        let dt = now.duration_since(last_edge).as_micros() as u32;
        info!("NEC IR edge: dt={}µs", dt);
        last_edge = now;

        // Active-low receiver: every edge toggles the level.
        level_low = !level_low;

        let (decoder_state0, ir_nec_event, last_code0) =
            feed(decoder_state, level_low, dt, last_code);
        decoder_state = decoder_state0;
        last_code = last_code0;

        if let Some(ir_event) = ir_nec_event {
            notifier.send(ir_event).await;
        }
    }
}

// ===== Decoder (same timings/logic as your working example) =================

#[derive(Copy, Clone, Debug, PartialEq)]
enum DecoderState {
    Idle,
    LdrLow,
    LdrHigh,
    BitLow { n: u8, v: u32 },
    BitHigh { n: u8, v: u32 },
    RepeatTail,
}

#[inline]
fn inr(x: u32, r: (u32, u32)) -> bool {
    x >= r.0 && x <= r.1
}
#[inline]
fn nec_ok(f: u32) -> Option<(u8, u8)> {
    let a = (f & 0xFF) as u8;
    let an = ((f >> 8) & 0xFF) as u8;
    let c = ((f >> 16) & 0xFF) as u8;
    let cn = ((f >> 24) & 0xFF) as u8;
    ((a ^ an) == 0xFF && (c ^ cn) == 0xFF).then_some((a, c))
}

// µs windows
const GLITCH: u32 = 120;
const LDR_LOW: (u32, u32) = (7_500, 10_500);
const LDR_HIGH: (u32, u32) = (3_700, 5_300);
const REP_HIGH: (u32, u32) = (1_750, 2_750);
const BIT_LOW: (u32, u32) = (360, 760);
const BIT0_HIGH: (u32, u32) = (310, 810);
const BIT1_HIGH: (u32, u32) = (1_190, 2_190);

// cmk move into an impl
fn feed(
    mut decoder_state: DecoderState,
    level_low: bool,
    dt: u32,
    mut last_code: Option<(u8, u8)>,
) -> (DecoderState, Option<IrNecEvent>, Option<(u8, u8)>) {
    if dt < GLITCH {
        return (decoder_state, None, last_code);
    }
    use DecoderState::*;
    match decoder_state {
        Idle => {
            if level_low {
                decoder_state = LdrLow;
            }
        }
        LdrLow => {
            if !level_low && inr(dt, LDR_LOW) {
                decoder_state = LdrHigh;
            } else {
                decoder_state = Idle;
            }
        }
        LdrHigh => {
            if level_low && inr(dt, LDR_HIGH) {
                decoder_state = BitLow { n: 0, v: 0 };
            } else if level_low && inr(dt, REP_HIGH) {
                decoder_state = RepeatTail;
            } else {
                decoder_state = Idle;
            }
        }
        RepeatTail => {
            if !level_low && inr(dt, BIT_LOW) {
                let out = last_code.map(|(a, c)| IrNecEvent::Repeat { addr: a, cmd: c });
                decoder_state = Idle;
                return (decoder_state, out, last_code);
            } else {
                decoder_state = Idle;
            }
        }
        BitLow { n, v } => {
            if !level_low && inr(dt, BIT_LOW) {
                decoder_state = BitHigh { n, v };
            } else {
                decoder_state = Idle;
            }
        }
        BitHigh { n, mut v } => {
            if level_low && inr(dt, BIT1_HIGH) {
                v |= 1u32 << n;
            } else if !(level_low && inr(dt, BIT0_HIGH)) {
                decoder_state = Idle;
                return (decoder_state, None, last_code);
            }

            let n2 = n + 1;
            if n2 == 32 {
                if let Some((a, c)) = nec_ok(v) {
                    last_code = Some((a, c));
                    decoder_state = Idle;
                    return (
                        decoder_state,
                        Some(IrNecEvent::Press { addr: a, cmd: c }),
                        last_code,
                    );
                } else {
                    decoder_state = Idle;
                }
            } else {
                decoder_state = BitLow { n: n2, v };
            }
        }
    }
    (decoder_state, None, last_code)
}
