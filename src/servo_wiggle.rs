//! A device abstraction for oscillating a single servo between two angles.
//!
//! See [`WigglingServo`] for usage and examples.

use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};

use crate::servo::Servo;

const WIGGLE_DELTA_DEGREES: i32 = 10;
const WIGGLE_PERIOD: Duration = Duration::from_millis(500);

/// Commands sent to the wiggling servo device.
struct WiggleCommand {
    degrees: i32,
    mode: WiggleMode,
}

/// Static resources for [`WigglingServo`].
pub struct WigglingServoStatic {
    commands: Channel<CriticalSectionRawMutex, WiggleCommand, 4>,
}

impl WigglingServoStatic {
    /// Create static resources for the wiggling servo device.
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            commands: Channel::new(),
        }
    }
}

/// Determines how the servo should move.
#[derive(Clone, Copy, Debug, defmt::Format, PartialEq, Eq)]
pub enum WiggleMode {
    /// Hold the servo at a fixed angle.
    Still,
    /// Oscillate the servo around the target angle.
    Wiggle,
}

/// A device abstraction that drives a single servo with optional wiggle animation.
///
/// Use [`WigglingServo::set`] to update the target angle and whether it should wiggle.
pub struct WigglingServo {
    commands: &'static Channel<CriticalSectionRawMutex, WiggleCommand, 4>,
}

impl WigglingServo {
    /// Create the wiggling servo device and spawn its task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be spawned.
    #[must_use = "Device must be kept alive to drive the servo task"]
    pub fn new(
        wiggling_servo_static: &'static WigglingServoStatic,
        servo: Servo<'static>,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = device_loop(wiggling_servo_static, servo)?;
        spawner.spawn(token);
        Ok(Self {
            commands: &wiggling_servo_static.commands,
        })
    }

    /// Set the target angle (0..=180) and motion mode.
    ///
    /// If `mode` is [`WiggleMode::Wiggle`], the servo will oscillate ±10° around the
    /// target angle until updated again.
    pub async fn set(&self, degrees: i32, mode: WiggleMode) {
        assert!((0..=180).contains(&degrees));
        self.commands.send(WiggleCommand { degrees, mode }).await;
    }
}

#[embassy_executor::task]
async fn device_loop(
    wiggling_servo_static: &'static WigglingServoStatic,
    mut servo: Servo<'static>,
) -> ! {
    let mut base_degrees = 0;
    let mut mode = WiggleMode::Still;
    let mut wiggle_high = false;
    servo.set_degrees(base_degrees);

    loop {
        match mode {
            WiggleMode::Still => {
                let command = wiggling_servo_static.commands.receive().await;
                base_degrees = command.degrees;
                mode = command.mode;
                wiggle_high = false;
                servo.set_degrees(base_degrees);
            }
            WiggleMode::Wiggle => {
                let wiggle_degrees = wiggle(base_degrees, wiggle_high);
                wiggle_high = !wiggle_high;
                servo.set_degrees(wiggle_degrees);

                match select(
                    Timer::after(WIGGLE_PERIOD),
                    wiggling_servo_static.commands.receive(),
                )
                .await
                {
                    Either::First(_) => {
                        // Continue wiggling.
                    }
                    Either::Second(command) => {
                        base_degrees = command.degrees;
                        mode = command.mode;
                        wiggle_high = false;
                        servo.set_degrees(base_degrees);
                    }
                }
            }
        }
    }
}

#[inline]
fn wiggle(base_degrees: i32, up: bool) -> i32 {
    if up {
        (base_degrees + WIGGLE_DELTA_DEGREES).min(180)
    } else {
        (base_degrees - WIGGLE_DELTA_DEGREES).max(0)
    }
}
