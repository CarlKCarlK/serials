use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

use crate::{
    adjustable_clock::AdjustableClock,
    state_machine::{ones_digit, tens_digit},
    virtual_display::{VirtualDisplay, CELL_COUNT0},
};

// cmk the virtual prefix is annoying
pub struct VirtualClock(&'static ClockNotifier);

// cmk we need to distinguish between the notifier for the clock and the display
pub type ClockNotifier = Signal<CriticalSectionRawMutex, ClockMode>;

// cmk only CELL_COUNT0
impl VirtualClock {
    pub fn new(
        virtual_display: VirtualDisplay<CELL_COUNT0>,
        clock_notifier: &'static ClockNotifier,
        spawner: Spawner,
    ) -> Self {
        let virtual_clock = Self(clock_notifier);
        unwrap!(spawner.spawn(virtual_clock_task(virtual_display, clock_notifier)));
        virtual_clock
    }

    // cmk is this the standard way to create a new notifier?
    // cmk it will be annoying to have to create a new display before creating a new clock
    pub const fn new_notifier() -> ClockNotifier {
        Signal::new()
    }
}

// impl VirtualClock {
//     pub fn write_chars(&self, chars: [char; CELL_COUNT]) {
//         info!("write_chars: {:?}", chars);
//         self.0.signal(BitMatrix::from_chars(&chars));
//     }
// }

pub enum ClockMode {
    HhMm,
    MmSs,
    Ss,
    Mm,
    Hh,
}

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
    loop {
        let next_updated = match clock_mode {
            ClockMode::HhMm => {
                let (hours, minutes, _) = adjustable_clock.h_m_s();
                virtual_display.write_chars([
                    tens_digit(hours),
                    ones_digit(hours),
                    tens_digit(minutes),
                    ones_digit(minutes),
                ]);
                // cmk not necessary exactly 60 seconds
                Duration::from_secs(60) // const
            }
            ClockMode::MmSs => {
                let (_, minutes, seconds) = adjustable_clock.h_m_s();
                virtual_display.write_chars([
                    tens_digit(minutes),
                    ones_digit(minutes),
                    tens_digit(seconds),
                    ones_digit(seconds),
                ]);
                Duration::from_secs(1) // const
            }
            _ => todo!(),
        };

        if let Either::Second(new_clock_mode) =
            select(Timer::after(next_updated), clock_notifier.wait()).await
        {
            clock_mode = new_clock_mode;
            continue;
        }
    }
}
