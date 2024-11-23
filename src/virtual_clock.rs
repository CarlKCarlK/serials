use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

const BLINK_OFF_DELAY: Duration = Duration::from_millis(50); // const cmk
const BLINK_ON_DELAY: Duration = Duration::from_millis(150); // const cmk

use crate::{
    adjustable_clock::AdjustableClock,
    state_machine::{ones_digit, tens_digit, tens_hours, ONE_MINUTE},
    virtual_display::{VirtualDisplay, CELL_COUNT0},
};

// cmk the virtual prefix is annoying
pub struct VirtualClock(&'static ClockNotifier);

// cmk we need to distinguish between the notifier for the clock and the display
pub type ClockNotifier = Channel<CriticalSectionRawMutex, ClockUpdate, 4>;

// cmk only CELL_COUNT0
impl VirtualClock {
    pub fn new(
        virtual_display: VirtualDisplay<CELL_COUNT0>,
        clock_notifier: &'static ClockNotifier,
        spawner: Spawner,
    ) -> Self {
        // cmk000 start the virtualDisplay, too
        let virtual_clock = Self(clock_notifier);
        unwrap!(spawner.spawn(virtual_clock_task(virtual_display, clock_notifier)));
        virtual_clock
    }

    // cmk 000 return the Signal for the VirtualDisplay, too.
    // cmk is this the standard way to create a new notifier?
    // cmk it will be annoying to have to create a new display before creating a new clock
    pub const fn new_notifier() -> ClockNotifier {
        Channel::new()
    }

    pub async fn set_mode(&self, clock_mode: ClockMode, blink_mode: BlinkMode) {
        self.0
            .send(ClockUpdate::SetMode {
                clock_mode,
                blink_mode,
            })
            .await;
    }

    pub async fn adjust_offset(&self, delta: Duration) {
        self.0.send(ClockUpdate::AdjustOffset(delta)).await;
    }

    pub async fn reset_seconds(&self) {
        self.0.send(ClockUpdate::ResetSeconds).await;
    }
}

pub enum ClockUpdate {
    SetMode {
        clock_mode: ClockMode,
        blink_mode: BlinkMode,
    },
    AdjustOffset(Duration),
    ResetSeconds,
}

pub enum ClockMode {
    HhMm,
    MmSs,
    Ss,
    SsIs00,
    Mm,
    Hh,
}

pub enum BlinkMode {
    NoBlink,
    BlinkingAndOn,
    BlinkingButOff,
}

#[allow(clippy::too_many_lines)] // cmk
#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn virtual_clock_task(
    // cmk does this need 'static? What does it mean?
    virtual_display: VirtualDisplay<CELL_COUNT0>,
    clock_notifier: &'static ClockNotifier,
) -> ! {
    // cmk blink mode?
    let mut adjustable_clock = AdjustableClock::default();
    let mut clock_mode = ClockMode::MmSs;
    let mut blink_mode = BlinkMode::NoBlink;
    loop {
        let update = match blink_mode {
            BlinkMode::BlinkingButOff => {
                virtual_display.write_chars([' ', ' ', ' ', ' ']);
                blink_mode = BlinkMode::BlinkingAndOn;
                BLINK_OFF_DELAY
            }
            BlinkMode::BlinkingAndOn | BlinkMode::NoBlink => {
                let mut update = match clock_mode {
                    ClockMode::HhMm => {
                        let (hours, minutes, _, update) =
                            adjustable_clock.h_m_s_update(Duration::from_secs(60)); // const
                                                                                    // cmk return the char array and apply to the VirtualDisplay in one place.
                        virtual_display.write_chars([
                            tens_hours(hours),
                            ones_digit(hours),
                            tens_digit(minutes),
                            ones_digit(minutes),
                        ]);
                        update
                    }
                    ClockMode::MmSs => {
                        let (_, minutes, seconds, update) =
                            adjustable_clock.h_m_s_update(Duration::from_secs(1)); // const
                        virtual_display.write_chars([
                            tens_digit(minutes),
                            ones_digit(minutes),
                            tens_digit(seconds),
                            ones_digit(seconds),
                        ]);
                        update
                    }
                    ClockMode::Ss => {
                        let (_, _, seconds, update) =
                            adjustable_clock.h_m_s_update(Duration::from_secs(1)); // const
                        virtual_display.write_chars([
                            ' ',
                            tens_digit(seconds),
                            ones_digit(seconds),
                            ' ',
                        ]);
                        update
                    }
                    ClockMode::SsIs00 => {
                        virtual_display.write_chars([' ', '0', '0', ' ']);
                        Duration::from_secs(60 * 60 * 24) // cmk const
                    }
                    ClockMode::Mm => {
                        let (_, minutes, _, update) =
                            adjustable_clock.h_m_s_update(Duration::from_secs(60)); // const
                        virtual_display.write_chars([
                            ' ',
                            ' ',
                            tens_digit(minutes),
                            ones_digit(minutes),
                        ]);
                        update
                    }
                    ClockMode::Hh => {
                        let (hours, _, _, update) =
                            adjustable_clock.h_m_s_update(Duration::from_secs(60 * 60)); // const
                        virtual_display.write_chars([
                            tens_hours(hours),
                            ones_digit(hours),
                            ' ',
                            ' ',
                        ]);
                        update
                    }
                };
                if let BlinkMode::BlinkingAndOn = blink_mode {
                    update = BLINK_ON_DELAY.min(update);
                    blink_mode = BlinkMode::BlinkingButOff;
                }
                update
            }
        };

        info!("Sleep for {:?}", update);
        if let Either::Second(clock_update) =
            select(Timer::after(update), clock_notifier.receive()).await
        {
            match clock_update {
                ClockUpdate::AdjustOffset(delta) => {
                    adjustable_clock += delta;
                }
                ClockUpdate::SetMode {
                    clock_mode: new_clock_mode,
                    blink_mode: new_blink_mode,
                } => {
                    clock_mode = new_clock_mode;
                    blink_mode = new_blink_mode;
                }
                ClockUpdate::ResetSeconds => {
                    let now_mod_minute = Duration::from_ticks(
                        adjustable_clock.now().as_ticks() % ONE_MINUTE.as_ticks(),
                    );
                    adjustable_clock += ONE_MINUTE - now_mod_minute;
                }
            }
            continue;
        }
    }
}
