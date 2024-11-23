use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};

use crate::{
    offset_time::OffsetTime,
    state_machine::{ones_digit, tens_digit, tens_hours, ONE_MINUTE},
    virtual_display::{BlinkMode, VirtualDisplay, CELL_COUNT0},
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

    pub async fn set_mode(&self, clock_mode: ClockMode) {
        self.0.send(ClockUpdate::SetMode { clock_mode }).await;
    }

    pub async fn adjust_offset(&self, delta: Duration) {
        self.0.send(ClockUpdate::AdjustOffset(delta)).await;
    }

    pub async fn reset_seconds(&self) {
        self.0.send(ClockUpdate::ResetSeconds).await;
    }
}

pub enum ClockUpdate {
    SetMode { clock_mode: ClockMode },
    AdjustOffset(Duration),
    ResetSeconds,
}

pub enum ClockMode {
    HhMm,
    MmSs,
    SsBlink,
    SsIs00,
    MmBlink,
    MmSolid,
    HhBlink,
    HhSolid,
}

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn virtual_clock_task(
    // cmk does this need 'static? What does it mean?
    virtual_display: VirtualDisplay<CELL_COUNT0>,
    clock_notifier: &'static ClockNotifier,
) -> ! {
    let mut offset_time = OffsetTime::default();
    let mut clock_mode = ClockMode::MmSs;
    loop {
        // Compute the display and time until the display change.
        // cmk0000 make a helper function for this match???
        let (chars, blink_mode, sleep_duration) = match &clock_mode {
            ClockMode::HhMm => handle_hh_mm(&offset_time),
            ClockMode::MmSs => handle_mm_ss(&offset_time),
            ClockMode::SsBlink => handle_ss_blink(&offset_time),
            ClockMode::SsIs00 => handle_ss_is00(&offset_time),
            ClockMode::MmBlink => handle_mm_blink(&offset_time),
            ClockMode::MmSolid => handle_mm_solid(&offset_time),
            ClockMode::HhBlink => handle_hh_blink(&offset_time),
            ClockMode::HhSolid => handle_hh_solid(&offset_time),
        };

        // Update the display
        virtual_display.write_chars(chars, blink_mode);

        // // Update blinking state and update the sleep duration.
        // blink_mode = match blink_mode {
        //     BlinkMode::BlinkingAndOn => {
        //         sleep_duration = BLINK_ON_DELAY.min(sleep_duration);
        //         BlinkMode::BlinkingButOff
        //     }
        //     BlinkMode::BlinkingButOff => BlinkMode::BlinkingAndOn,
        //     BlinkMode::NoBlink => BlinkMode::NoBlink,
        // };
        // cmk00000 move blink mode into the virtual display

        // Wait for a notification or for the sleep duration to elapse
        info!("Sleep for {:?}", sleep_duration);
        if let Either::First(notification) =
            select(clock_notifier.receive(), Timer::after(sleep_duration)).await
        {
            handle_notification(notification, &mut offset_time, &mut clock_mode);
        }
    }
}

fn handle_hh_mm(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    let (hours, minutes, _, update) = offset_time.h_m_s_update(Duration::from_secs(60));
    (
        [
            tens_hours(hours),
            ones_digit(hours),
            tens_digit(minutes),
            ones_digit(minutes),
        ],
        BlinkMode::Solid,
        update,
    )
}

fn handle_mm_ss(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    let (_, minutes, seconds, update) = offset_time.h_m_s_update(Duration::from_secs(1));
    (
        [
            tens_digit(minutes),
            ones_digit(minutes),
            tens_digit(seconds),
            ones_digit(seconds),
        ],
        BlinkMode::Solid,
        update,
    )
}

fn handle_ss_blink(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    let (_, _, seconds, update) = offset_time.h_m_s_update(Duration::from_secs(1));
    (
        [' ', tens_digit(seconds), ones_digit(seconds), ' '],
        BlinkMode::BlinkingAndOn,
        update,
    )
}

fn handle_ss_is00(_offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    (
        [' ', '0', '0', ' '],
        BlinkMode::Solid,
        Duration::from_secs(60 * 60 * 24),
    )
}

fn handle_mm_blink(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    let (_, minutes, _, update) = offset_time.h_m_s_update(Duration::from_secs(60));
    (
        [' ', ' ', tens_digit(minutes), ones_digit(minutes)],
        BlinkMode::BlinkingAndOn,
        update,
    )
}

fn handle_mm_solid(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    let (_, minutes, _, update) = offset_time.h_m_s_update(Duration::from_secs(60));
    (
        [' ', ' ', tens_digit(minutes), ones_digit(minutes)],
        BlinkMode::Solid,
        update,
    )
}

fn handle_hh_blink(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    let (hours, _, _, update) = offset_time.h_m_s_update(Duration::from_secs(60 * 60));
    (
        [tens_hours(hours), ones_digit(hours), ' ', ' '],
        BlinkMode::BlinkingAndOn,
        update,
    )
}

fn handle_hh_solid(offset_time: &OffsetTime) -> ([char; 4], BlinkMode, Duration) {
    let (hours, _, _, update) = offset_time.h_m_s_update(Duration::from_secs(60 * 60));
    (
        [tens_hours(hours), ones_digit(hours), ' ', ' '],
        BlinkMode::Solid,
        update,
    )
}

fn handle_notification(
    clock_update: ClockUpdate,
    offset_time: &mut OffsetTime,
    clock_mode: &mut ClockMode,
) {
    match clock_update {
        ClockUpdate::AdjustOffset(delta) => {
            *offset_time += delta;
        }
        ClockUpdate::SetMode {
            clock_mode: new_clock_mode,
        } => {
            *clock_mode = new_clock_mode;
        }
        ClockUpdate::ResetSeconds => {
            let now_mod_minute =
                Duration::from_ticks(offset_time.now().as_ticks() % ONE_MINUTE.as_ticks());
            *offset_time += ONE_MINUTE - now_mod_minute;
        }
    }
}
