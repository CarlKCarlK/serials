//! A device abstraction for servo motors using hardware PWM.

use defmt::info;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::pwm::{Config, Pwm};

pub const SERVO_PERIOD_US: u16 = 20_000; // 20 ms

/// Convenience macros to create a servo in one line.
///
/// The macro expands to call `Pwm::new_output_a()` (or `_b()`) internally,
/// so you don't need to create the PWM manually. The type checker will verify
/// that the slice and pin are compatible with the chosen channel (A or B).
///
/// # Examples
/// ```ignore
/// // Channel A servo on GPIO0 (PWM slice 0)
/// let mut servo_a = servo_a!(p.PWM_SLICE0, p.PIN_0, 500, 2500);
///
/// // Channel B servo on GPIO1 (PWM slice 0)
/// let mut servo_b = servo_b!(p.PWM_SLICE0, p.PIN_1, 500, 2500);
/// ```

/// A device abstraction that creates a servo on PWM channel A.
#[macro_export]
macro_rules! servo_a {
    ($slice:expr, $pin:expr, $min_us:expr, $max_us:expr) => {
        $crate::servo::Servo::new(
            embassy_rp::pwm::Pwm::new_output_a($slice, $pin, embassy_rp::pwm::Config::default()),
            $crate::servo::ServoChannel::A,
            $min_us,
            $max_us,
        )
    };
}

/// A device abstraction that creates a servo on PWM channel B.
#[macro_export]
macro_rules! servo_b {
    ($slice:expr, $pin:expr, $min_us:expr, $max_us:expr) => {
        $crate::servo::Servo::new(
            embassy_rp::pwm::Pwm::new_output_b($slice, $pin, embassy_rp::pwm::Config::default()),
            $crate::servo::ServoChannel::B,
            $min_us,
            $max_us,
        )
    };
}

/// A device abstraction for SG90/FS90R servo motors using hardware PWM.
pub struct Servo<'d> {
    pwm: Pwm<'d>,
    cfg: Config, // Store config to avoid recreating default (which resets divider)
    top: u16,
    min_us: u16,
    max_us: u16,
    channel: ServoChannel, // Track which channel (A or B) this servo uses
}

/// Which PWM channel the servo is on.
#[derive(Debug, Clone, Copy)]
pub enum ServoChannel {
    A,
    B,
}

impl<'d> Servo<'d> {
    /// Create on a PWM output channel, accepting pre-configured Pwm.
    /// e.g.: Servo::new(Pwm::new_output_a(p.PWM_SLICE0, p.PIN_0, Config::default()), ServoChannel::A, 500, 2500)
    pub fn new(pwm: Pwm<'d>, channel: ServoChannel, min_us: u16, max_us: u16) -> Self {
        Self::init(pwm, channel, min_us, max_us)
    }

    /// Configure PWM and initialize servo. Internal shared logic.
    fn init(mut pwm: Pwm<'d>, channel: ServoChannel, min_us: u16, max_us: u16) -> Self {
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

        // Set the appropriate compare register based on channel
        match channel {
            ServoChannel::A => cfg.compare_a = 1500, // start ~center
            ServoChannel::B => cfg.compare_b = 1500, // start ~center
        }

        cfg.enable = true; // Enable PWM output
        pwm.set_config(&cfg);

        info!(
            "servo clk={}Hz div={}.{} top={}",
            clk, div_int, div_frac, top
        );

        let mut s = Self {
            pwm,
            cfg, // Store config to avoid losing divider on reconfiguration
            top,
            min_us,
            max_us,
            channel,
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
        let us = self.min_us as u32 + (d as u32) * (self.max_us as u32 - self.min_us as u32) / 180;
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
        match self.channel {
            ServoChannel::A => self.cfg.compare_a = us,
            ServoChannel::B => self.cfg.compare_b = us,
        }
        self.pwm.set_config(&self.cfg);
    }

    /// Stop the slice (most servos relax).
    pub fn disable(&mut self) {
        self.cfg.enable = false;
        self.pwm.set_config(&self.cfg);
    }

    /// Resume output (keeps last duty). Call `center()` if you prefer.
    pub fn enable(&mut self) {
        self.cfg.enable = true;
        self.pwm.set_config(&self.cfg);
    }
}
