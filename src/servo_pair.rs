//! Dual servo control using both channels of a single PWM slice.

use defmt::info;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::pwm::{Config, Pwm};

const SERVO_PERIOD_US: u16 = 20_000; // 20 ms

/// A device abstraction for controlling two SG90 servos on the same PWM slice.
///
/// This allows you to control two servos using both channels (A and B) of a single
/// PWM slice, sharing the same timing configuration.
///
/// # Examples
/// ```
/// # use embassy_rp::pwm::{Config, Pwm};
/// # use serials::servo_pair::ServoPair;
/// # embassy_rp::pac::Peripherals::take();
/// # let p = unsafe { embassy_rp::Peripherals::steal() };
/// let pwm = Pwm::new_output_ab(p.PWM_SLICE0, p.PIN_0, p.PIN_1, Config::default());
/// let mut servos = ServoPair::new(pwm, 500, 2500, 500, 2500);
///
/// servos.set_degrees_a(45);
/// servos.set_degrees_b(90);
/// ```
pub struct ServoPair<'d> {
    pwm: Pwm<'d>,
    cfg: Config,
    top: u16,
    min_us_a: u16,
    max_us_a: u16,
    min_us_b: u16,
    max_us_b: u16,
}

impl<'d> ServoPair<'d> {
    /// Create a pair of servos on both channels of a PWM slice.
    ///
    /// # Arguments
    /// * `pwm` - A `Pwm` instance created with `Pwm::new_output_ab()`
    /// * `min_us_a` - Minimum pulse width in microseconds for servo A (typically 500)
    /// * `max_us_a` - Maximum pulse width in microseconds for servo A (typically 2500)
    /// * `min_us_b` - Minimum pulse width in microseconds for servo B (typically 500)
    /// * `max_us_b` - Maximum pulse width in microseconds for servo B (typically 2500)
    pub fn new(
        mut pwm: Pwm<'d>,
        min_us_a: u16,
        max_us_a: u16,
        min_us_b: u16,
        max_us_b: u16,
    ) -> Self {
        let clk = clk_sys_freq() as u64;
        let mut div_int = (clk / 1_000_000).clamp(1, 255) as u16;
        let rem = clk.saturating_sub(div_int as u64 * 1_000_000);
        let mut div_frac = ((rem * 16 + 500_000) / 1_000_000).clamp(0, 15) as u8;
        if div_frac == 16 {
            div_frac = 0;
            div_int = (div_int + 1).min(255);
        }

        let top = SERVO_PERIOD_US - 1;

        let mut cfg = Config::default();
        cfg.top = top;
        cfg.phase_correct = false;
        cfg.divider = (div_int as u8).into();
        cfg.compare_a = 1500; // center
        cfg.compare_b = 1500; // center
        cfg.enable = true;
        pwm.set_config(&cfg);

        info!(
            "servo_pair clk={}Hz div={}.{} top={}",
            clk, div_int, div_frac, top
        );

        let mut s = Self {
            pwm,
            cfg,
            top,
            min_us_a,
            max_us_a,
            min_us_b,
            max_us_b,
        };
        s.center_a();
        s.center_b();
        s
    }

    /// Center servo A.
    pub fn center_a(&mut self) {
        self.set_pulse_us_a(self.min_us_a + (self.max_us_a - self.min_us_a) / 2);
    }

    /// Center servo B.
    pub fn center_b(&mut self) {
        self.set_pulse_us_b(self.min_us_b + (self.max_us_b - self.min_us_b) / 2);
    }

    /// Set servo A position in degrees 0..=180.
    pub fn set_degrees_a(&mut self, deg: i32) {
        let d = deg.clamp(0, 180) as u16;
        let us =
            self.min_us_a as u32 + (d as u32) * (self.max_us_a as u32 - self.min_us_a as u32) / 180;
        info!("ServoA set_degrees({}) -> {}µs", deg, us);
        self.set_pulse_us_a(us as u16);
    }

    /// Set servo B position in degrees 0..=180.
    pub fn set_degrees_b(&mut self, deg: i32) {
        let d = deg.clamp(0, 180) as u16;
        let us =
            self.min_us_b as u32 + (d as u32) * (self.max_us_b as u32 - self.min_us_b as u32) / 180;
        info!("ServoB set_degrees({}) -> {}µs", deg, us);
        self.set_pulse_us_b(us as u16);
    }

    fn set_pulse_us_a(&mut self, mut us: u16) {
        if us > self.top {
            us = self.top;
        }
        self.cfg.compare_a = us;
        self.pwm.set_config(&self.cfg);
    }

    fn set_pulse_us_b(&mut self, mut us: u16) {
        if us > self.top {
            us = self.top;
        }
        self.cfg.compare_b = us;
        self.pwm.set_config(&self.cfg);
    }

    /// Stop sending control signals to both servos.
    pub fn disable(&mut self) {
        self.cfg.enable = false;
        self.pwm.set_config(&self.cfg);
    }

    /// Resume sending control signals to both servos.
    pub fn enable(&mut self) {
        self.cfg.enable = true;
        self.pwm.set_config(&self.cfg);
    }
}
