//! A device abstraction for oscillating a single servo between two angles.
//!
//! See [`WigglingServo`] for usage and examples.

use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use heapless::Vec;

use crate::servo::Servo;

const WIGGLE_DELTA_DEGREES: i32 = 10;
const WIGGLE_PERIOD: Duration = Duration::from_millis(500);

/// Commands sent to the wiggling servo device.
enum WiggleCommand {
    Set { degrees: i32, mode: WiggleMode },
    Animate { steps: AnimateSequence },
}

#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct AnimateStep {
    pub degrees: i32,
    pub hold_duration: Duration,
}

type AnimateSequence = Vec<AnimateStep, MAX_ANIMATE_STEPS>;
const MAX_ANIMATE_STEPS: usize = 16;

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
    /// Create static resources for a wiggling servo.
    #[must_use]
    pub const fn new_static() -> WigglingServoStatic {
        WigglingServoStatic::new_static()
    }

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
        self.commands
            .send(WiggleCommand::Set { degrees, mode })
            .await;
    }

    /// Animate the servo through a sequence of angles with per-step hold durations.
    /// The sequence repeats until interrupted by a new command.
    pub async fn animate(&self, steps: &[AnimateStep]) {
        assert!(!steps.is_empty(), "animate requires at least one step");
        let mut sequence: AnimateSequence = Vec::new();
        for step in steps {
            assert!((0..=180).contains(&step.degrees));
            assert!(
                step.hold_duration.as_micros() > 0,
                "hold duration must be positive"
            );
            sequence.push(*step).expect("animate sequence fits");
        }

        self.commands
            .send(WiggleCommand::Animate { steps: sequence })
            .await;
    }
}

#[embassy_executor::task(pool_size = 2)]
async fn device_loop(
    wiggling_servo_static: &'static WigglingServoStatic,
    mut servo: Servo<'static>,
) -> ! {
    let mut base_degrees = 0;
    let mut mode = WiggleMode::Still;
    let mut wiggle_high = false;
    servo.set_degrees(base_degrees);

    let mut pending_command: Option<WiggleCommand> = None;

    loop {
        let command = if let Some(command) = pending_command.take() {
            command
        } else {
            match mode {
                WiggleMode::Still => wiggling_servo_static.commands.receive().await,
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
                        Either::First(_) => continue,
                        Either::Second(command) => command,
                    }
                }
            }
        };

        match command {
            WiggleCommand::Set {
                degrees,
                mode: new_mode,
            } => {
                base_degrees = degrees;
                mode = new_mode;
                wiggle_high = false;
                servo.set_degrees(base_degrees);
            }
            WiggleCommand::Animate { steps } => {
                mode = WiggleMode::Still;
                wiggle_high = false;
                let final_target = steps.last().map(|step| step.degrees);
                if let Some(command) = run_animation(
                    steps,
                    &mut servo,
                    &wiggling_servo_static.commands,
                    &mut base_degrees,
                )
                .await
                {
                    pending_command = Some(command);
                } else if let Some(target) = final_target {
                    base_degrees = target;
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
async fn run_animation(
    steps: AnimateSequence,
    servo: &mut Servo<'static>,
    commands: &Channel<CriticalSectionRawMutex, WiggleCommand, 4>,
    current_degrees: &mut i32,
) -> Option<WiggleCommand> {
    loop {
        for step in &steps {
            if *current_degrees != step.degrees {
                servo.set_degrees(step.degrees);
                *current_degrees = step.degrees;
            }
            match select(Timer::after(step.hold_duration), commands.receive()).await {
                Either::First(_) => {}
                Either::Second(command) => return Some(command),
            }
        }
    }
}
