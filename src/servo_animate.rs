//! A device abstraction for animating a loop of servo actions.
//!
//! See [`ServoAnimate`] for usage and examples, and [`Servo`] for servo setup helpers.

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
    Set { degrees: u16 },
    Animate { steps: AnimateSequence },
}

// cmk should this be Frame?
/// A single animation step: hold `degrees` for `duration`.
///
/// See [`ServoAnimate`] for a full example.
#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct Step {
    pub degrees: u16,
    pub duration: Duration,
}

/// Build a linear sequence of `AnimateStep`s from `start_degrees` to `end_degrees` over
/// `total_duration` split into `N` steps (inclusive of endpoints).
///
/// See [`ServoAnimate`] for a complete example of building sequences and running them.
#[must_use]
pub fn linear<const N: usize>(
    start_degrees: u16,
    end_degrees: u16,
    total_duration: Duration,
) -> [Step; N] {
    assert!(N > 0, "at least one step required");
    assert!((0..=180).contains(&start_degrees));
    assert!((0..=180).contains(&end_degrees));
    assert!(
        total_duration.as_micros() > 0,
        "total duration must be positive"
    );
    let step_duration = total_duration / (N as u32);
    let delta = i32::from(end_degrees) - i32::from(start_degrees);
    let denom = i32::try_from(((N - 1) as i32).max(1)).expect("denom fits in i32");
    array::from_fn(|idx| {
        let degrees = if N == 1 {
            start_degrees
        } else {
            let step = delta * i32::try_from(idx).expect("index fits") / denom;
            u16::try_from(i32::from(start_degrees) + step).expect("angle fits")
        };
        Step {
            degrees,
            duration: step_duration,
        }
    })
}

type AnimateSequence = Vec<Step, 16>;

/// Concatenate arrays of animation [`Step`] values into a single sequence.
///
/// Provide the capacity as a const generic and pass slices of step arrays.
/// Import with `use device_kit::servo_animate::concat_steps;`.
#[must_use]
pub fn concat_steps<const CAP: usize>(sequences: &[&[Step]]) -> Vec<Step, CAP> {
    let mut out: Vec<Step, CAP> = Vec::new();
    for sequence in sequences {
        for step in *sequence {
            out.push(*step).expect("sequence fits");
        }
    }
    out
}
pub use crate::servo::{servo_even, servo_odd};

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

// cmk should step have a ::new?

/// A device abstraction that drives a single servo with scripted animation sequences.
///
/// See [`Servo`] for servo setup guidance and [`ServoAnimate`] for usage.
///
/// # Example
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// use device_kit::servo_animate::{concat_steps, linear, ServoAnimate, ServoAnimateStatic, Step, servo_even};
/// use embassy_time::Duration;
/// # #[panic_handler]
/// # fn panic(_info: &core::panic::PanicInfo) -> ! { loop {} }
///
/// async fn demo(p: embassy_rp::Peripherals, spawner: embassy_executor::Spawner) {
///     static SERVO_ANIMATE_STATIC: ServoAnimateStatic = ServoAnimate::new_static();
///     let servo = ServoAnimate::new(
///         &SERVO_ANIMATE_STATIC,
///         servo_even!(p.PIN_0, p.PWM_SLICE0, 500, 2500),
///         spawner,
///     )
///     .unwrap();
///
///     // Sweep down from 180 to 0 over 5 seconds, hold, then repeat.
///     const FIVE_SECONDS: Duration = Duration::from_secs(5);
///     const HALF_SECOND: Duration = Duration::from_millis(500);
///     let sweep = linear::<11>(180, 0, FIVE_SECONDS);
///     let sequence = concat_steps::<16>(&[
///         &sweep,
///         &[
///             Step {
///                 degrees: 0,
///                 duration: HALF_SECOND,
///             },
///         ],
///     ]);
///     servo.animate(&sequence).await;
/// }
/// ```
pub struct ServoAnimate {
    commands: &'static Channel<CriticalSectionRawMutex, AnimateCommand, 4>,
}

impl ServoAnimate {
    /// Create static resources for a servo animator.
    #[must_use]
    pub const fn new_static() -> ServoAnimateStatic {
        ServoAnimateStatic::new_static()
    }

    /// Create the servo animator and spawn its task. See [`ServoAnimate`] for a full example.
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

    /// Set the target angle (0..=180). See [`ServoAnimate`] for usage.
    pub async fn set(&self, degrees: u16) {
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
    let mut base_degrees: u16 = 0;
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
    current_degrees: &mut u16,
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
