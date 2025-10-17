#![no_std]
#![no_main]

use defmt::{info, warn};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::select;
use embassy_rp::gpio::{Input, Pull};
use embassy_time::{ Instant, Timer};
use panic_probe as _;

// --- NEC edge-driven decoder -----------------------------------------------
#[derive(Copy, Clone, Debug, PartialEq)]
enum S { Idle, LdrLow, LdrHigh, BitLow{n:u8,v:u32}, BitHigh{n:u8,v:u32}, RepeatTail }

const GLITCH: u32 = 120;
const LDR_LOW:   (u32,u32) = (7_500, 10_500);
const LDR_HIGH:  (u32,u32) = (3_700,  5_300);
const REP_HIGH:  (u32,u32) = (1_750,  2_750);
const BIT_LOW:   (u32,u32) = (  360,    760);
const BIT0_HIGH: (u32,u32) = (  310,    810);
const BIT1_HIGH: (u32,u32) = (1_190,  2_190);

#[inline] fn in_rng(x:u32, r:(u32,u32))->bool { x>=r.0 && x<=r.1 }
#[inline] fn nec_ok(frame:u32)->Option<(u8,u8)>{
    let a  =(frame&0xFF) as u8;
    let an =((frame>>8)&0xFF) as u8;
    let c  =((frame>>16)&0xFF) as u8;
    let cn =((frame>>24)&0xFF) as u8;
    if a^an==0xFF && c^cn==0xFF { Some((a,c)) } else { None }
}

struct Nec {
    st: S,
    last_code: Option<(u8,u8)>,
}

impl Nec {
    fn new() -> Self { Self { st:S::Idle, last_code:None } }

    /// Feed *one* edge: `level_low` is the new level after the edge, `dt` is time since previous edge.
    /// Returns Some(addr,cmd) when a full frame (or repeat) is decoded.
    fn feed(&mut self, level_low: bool, dt: u32) -> Option<(u8,u8)> {
        if dt < GLITCH { return None; }

        match self.st {
            S::Idle => {
                // We just ended a long idle with a falling edge → start leader low
                if level_low { self.st = S::LdrLow; }
            }
            S::LdrLow => {
                // Rising edge after ~9ms low
                if !level_low && in_rng(dt, LDR_LOW) { self.st = S::LdrHigh; }
                else { self.st = S::Idle; }
            }
            S::LdrHigh => {
                // Falling edge after ~4.5ms high → data, or ~2.25ms → repeat
                if level_low && in_rng(dt, LDR_HIGH) { self.st = S::BitLow{n:0,v:0}; }
                else if level_low && in_rng(dt, REP_HIGH) { self.st = S::RepeatTail; }
                else { self.st = S::Idle; }
            }
            S::RepeatTail => {
                // Rising edge after ~560us low completes repeat
                if !level_low && in_rng(dt, BIT_LOW) {
                    let out = self.last_code;
                    self.st = S::Idle;
                    return out; // repeat last code
                } else { self.st = S::Idle; }
            }
            S::BitLow{n,v} => {
                // Rising edge after ~560us low → measure high for bit value
                if !level_low && in_rng(dt, BIT_LOW) { self.st = S::BitHigh{n,v}; }
                else { self.st = S::Idle; }
            }
            S::BitHigh{n,mut v} => {
                // Falling edge ends the high: classify 0/1
                if level_low && in_rng(dt, BIT1_HIGH) { v |= 1u32 << n; }
                else if !(level_low && in_rng(dt, BIT0_HIGH)) { self.st = S::Idle; return None; }

                let n2 = n+1;
                if n2 == 32 {
                    if let Some(code) = nec_ok(v) {
                        self.last_code = Some(code);
                        self.st = S::Idle;
                        return Some(code);
                    } else { self.st = S::Idle; }
                } else {
                    self.st = S::BitLow{n:n2, v};
                }
            }
        }
        None
    }
}


#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());
    // Most IR receiver modules idle HIGH; Pull::Up makes the idle solid.
 

    defmt::info!("edge sniffer on GP6 (active-low expected)");

 let mut ir = embassy_rp::gpio::Input::new(p.PIN_6, embassy_rp::gpio::Pull::Up);
let mut last = embassy_time::Instant::now();
let mut level_low = ir.is_low(); // shadow
let mut nec = Nec::new();

loop {
    ir.wait_for_any_edge().await;
    // flip shadow (edges always toggle)
    level_low = !level_low;

    let now = embassy_time::Instant::now();
    let dt = now.duration_since(last).as_micros() as u32;
    last = now;

    if let Some((addr, cmd)) = nec.feed(level_low, dt) {
        defmt::info!("NEC addr=0x{:02X} cmd=0x{:02X}", addr, cmd);
    }
}
}