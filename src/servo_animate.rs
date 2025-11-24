//! A device abstraction for driving a single servo with scripted animations.
//!
//! See [`ServoAnimate`] for usage and examples.

use core::array;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use heapless::Vec;

use crate::servo::Servo;

/// Commands sent to the servo animate device.
enum AnimateCommand {
    Set { degrees: i32 },
    Animate { steps: AnimateSequence },
}

#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct Step {
    pub degrees: i32,
    pub duration: Duration,
}

/// Build a linear sequence of `AnimateStep`s from `start_degrees` to `end_degrees` over
/// `total_duration` split into `N` steps (inclusive of endpoints).
#[must_use]
pub fn linear<const N: usize>(
    start_degrees: i32,
    end_degrees: i32,
    total_duration: Duration,
) -> [Step; N] {
    assert!(N > 0, "at least one step required");
    // cmk if must be 0> then why i32 and not u32?
    assert!((0..=180).contains(&start_degrees));
    assert!((0..=180).contains(&end_degrees));
    assert!(
        total_duration.as_micros() > 0,
        "total duration must be positive"
    );
    let step_duration = total_duration / (N as u32);
    let delta = end_degrees - start_degrees;
    let denom = i32::try_from(((N - 1) as i32).max(1)).expect("denom fits in i32");
    array::from_fn(|idx| {
        let angle = if N == 1 {
            start_degrees
        } else {
            start_degrees + delta * i32::try_from(idx).expect("index fits") / denom
        };
        Step {
            degrees: angle,
            duration: step_duration,
        }
    })
}

type AnimateSequence = Vec<Step, 16>;

/// Macro to concatenate fixed-size arrays of `Step` without unsafe or nightly features.
/// Use the `cap = N` form to set the capacity of the temporary buffer.
#[macro_export]
macro_rules! servo_animate_concat {
    (cap = $cap:expr, $first:expr $(, $rest:expr)+ $(,)?) => {{
        let mut out: heapless::Vec<serials::servo_animate::Step, { $cap }> = heapless::Vec::new();
        let sequences: &[&[serials::servo_animate::Step]] = &[ $first $(, $rest)+ ];
        for seq in sequences {
            for step in *seq {
                out.push(*step).expect("sequence fits");
            }
        }
        out
    }};
}
pub use crate::servo_animate_concat as concat;

/// Static resources for [`ServoAnimate`].
pub struct ServoAnimateStatic {
    commands: Channel<CriticalSectionRawMutex, AnimateCommand, 4>,
}

impl ServoAnimateStatic {
    /// Create static resources for the servo animate device.
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            commands: Channel::new(),
        }
    }
}

/// A device abstraction that drives a single servo with scripted animation sequences.
pub struct ServoAnimate {
    commands: &'static Channel<CriticalSectionRawMutex, AnimateCommand, 4>,
}

impl ServoAnimate {
    /// Create static resources for a servo animator.
    #[must_use]
    pub const fn new_static() -> ServoAnimateStatic {
        ServoAnimateStatic::new_static()
    }

    /// Create the wiggling servo device and spawn its task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be spawned.
    #[must_use = "Device must be kept alive to drive the servo task"]
    pub fn new(
        servo_animate_static: &'static ServoAnimateStatic,
        servo: Servo<'static>,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = device_loop(servo_animate_static, servo)?;
        spawner.spawn(token);
        Ok(Self {
            commands: &servo_animate_static.commands,
        })
    }

    /// Set the target angle (0..=180).
    pub async fn set(&self, degrees: i32) {
        assert!((0..=180).contains(&degrees));
        self.commands.send(AnimateCommand::Set { degrees }).await;
    }

    /// Animate the servo through a sequence of angles with per-step hold durations.
    /// The sequence repeats until interrupted by a new command.
    pub async fn animate(&self, steps: &[Step]) {
        assert!(!steps.is_empty(), "animate requires at least one step");
        let mut sequence: AnimateSequence = Vec::new();
        for step in steps {
            assert!((0..=180).contains(&step.degrees));
            assert!(
                step.duration.as_micros() > 0,
                "hold duration must be positive"
            );
            sequence.push(*step).expect("animate sequence fits");
        }

        self.commands
            .send(AnimateCommand::Animate { steps: sequence })
            .await;
    }
}

#[embassy_executor::task(pool_size = 2)]
async fn device_loop(
    servo_animate_static: &'static ServoAnimateStatic,
    mut servo: Servo<'static>,
) -> ! {
    let mut base_degrees = 0;
    servo.set_degrees(base_degrees);

    let mut pending_command: Option<AnimateCommand> = None;

    loop {
        let command = if let Some(command) = pending_command.take() {
            command
        } else {
            servo_animate_static.commands.receive().await
        };

        match command {
            AnimateCommand::Set { degrees } => {
                base_degrees = degrees;
                servo.set_degrees(base_degrees);
            }
            AnimateCommand::Animate { steps } => {
                let final_target = steps.last().map(|step| step.degrees);
                if let Some(command) = run_animation(
                    steps,
                    &mut servo,
                    &servo_animate_static.commands,
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

async fn run_animation(
    steps: AnimateSequence,
    servo: &mut Servo<'static>,
    commands: &Channel<CriticalSectionRawMutex, AnimateCommand, 4>,
    current_degrees: &mut i32,
) -> Option<AnimateCommand> {
    loop {
        for step in &steps {
            if *current_degrees != step.degrees {
                servo.set_degrees(step.degrees);
                *current_degrees = step.degrees;
            }
            match select(Timer::after(step.duration), commands.receive()).await {
                Either::First(_) => {}
                Either::Second(command) => return Some(command),
            }
        }
    }
}
