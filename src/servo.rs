//! Hardware-PWM SG90/FS90R servo driver for RP2040 (Pico / Pico W) using Embassy.
//! - 50 Hz frame (20 ms)
//! - Clock-independent: computes divider from clk_sys so 1 tick ≈ 1 µs
//! - Updates duty WITHOUT reconfiguring the slice

use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::pwm::{Pwm, Config};
use defmt::info;

pub const SERVO_PERIOD_US: u16 = 20_000; // 20 ms

pub struct Servo<'d> {
    pwm: Pwm<'d>,
    cfg: Config,  // Store config to avoid recreating default (which resets divider)
    top: u16,
    min_us: u16,
    max_us: u16,
}

impl<'d> Servo<'d> {
    /// Create on a PWM output channel, accepting pre-configured Pwm.
    /// e.g.: Servo::new(Pwm::new_output_a(p.PWM_SLICE0, p.PIN_0, Config::default()), 500, 2500)
    pub fn new(pwm: Pwm<'d>, min_us: u16, max_us: u16) -> Self {
        Self::init(pwm, min_us, max_us)
    }

    /// Configure PWM and initialize servo. Internal shared logic.
    fn init(mut pwm: Pwm<'d>, min_us: u16, max_us: u16) -> Self {
        let clk = clk_sys_freq() as u64; // Hz
        // Aim for tick ≈ 1 µs: divider = clk_sys / 1_000_000 (with /16 fractional)
        let mut div_int = (clk / 1_000_000).clamp(1, 255) as u16;
        let rem = clk.saturating_sub(div_int as u64 * 1_000_000);
        let mut div_frac = ((rem * 16 + 500_000) / 1_000_000).clamp(0, 15) as u8;
        if div_frac == 16 {
            div_frac = 0;
            div_int = (div_int + 1).min(255);
        }

        let top = SERVO_PERIOD_US - 1; // 19999 -> 20_000 ticks/frame

        let mut cfg = Config::default();
        cfg.top = top;
        cfg.phase_correct = false; // edge-aligned => exact 1 µs steps
        // Apply divider: use the integer part as u8 which has a From impl
        cfg.divider = (div_int as u8).into();
        cfg.compare_a = 1500;      // start ~center if this is channel A
        cfg.enable = true;         // Enable PWM output
        pwm.set_config(&cfg);

        info!("servo clk={}Hz div={}.{} top={}", clk, div_int, div_frac, top);

        let mut s = Self {
            pwm,
            cfg,  // Store config to avoid losing divider on reconfiguration
            top,
            min_us,
            max_us,
        };
        s.center();
        s
    }

    /// Center (~midpoint of min/max).
    pub fn center(&mut self) {
        self.set_pulse_us(self.min_us + (self.max_us - self.min_us) / 2);
    }

    /// Set position in degrees 0..=180 (clamped) mapped into [min_us, max_us].
    pub fn set_degrees(&mut self, deg: i32) {
        let d = deg.clamp(0, 180) as u16;
        let us = self.min_us as u32
            + (d as u32) * (self.max_us as u32 - self.min_us as u32) / 180;
        info!("Servo set_degrees({}) -> {}µs", deg, us);
        self.set_pulse_us(us as u16);
    }

    /// Set raw pulse width in microseconds (clamped to frame).
    /// NOTE: only update the *compare* register; do not reconfigure the slice.
    pub fn set_pulse_us(&mut self, mut us: u16) {
        if us > self.top {
            us = self.top;
        }
        // One tick ≈ 1 µs, so compare = us.
        // CRITICAL: Update our stored config and reapply it WITH the divider intact.
        // This prevents the divider from being reset to default.
        self.cfg.compare_a = us;
        self.pwm.set_config(&self.cfg);
    }

    /// Stop the slice (most servos relax).
    pub fn disable(&mut self) {
        self.cfg.enable = false;
        self.pwm.set_config(&self.cfg);
    }

    // cmk000 what does this do?
    /// Resume output (keeps last duty). Call `center()` if you prefer.
    pub fn enable(&mut self) {
        self.cfg.enable = true;
        self.pwm.set_config(&self.cfg);
    }
}








