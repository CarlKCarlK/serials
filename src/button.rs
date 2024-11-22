// cmk what is this just button, but the display is 'virtual_display'?

use embassy_futures::select::{select, Either};
use embassy_rp::gpio::Input;
use embassy_time::{Duration, Instant, Timer};

// cmk why does brad pass AnyPin to the button constructor while I pass Input that was created in the pins module?

// cmk must this be static?
pub struct Button {
    pub inner: Input<'static>, // cmk remove this 'pub'
    time_down: Option<Instant>,
}

impl Button {
    #[must_use]
    pub fn new(button: Input<'static>) -> Self {
        Self {
            inner: button,
            time_down: None,
        }
    }

    pub fn is_up(&self) -> bool {
        self.inner.is_low()
    }

    pub async fn wait_for_press(&mut self) -> PressDuration {
        let how_much_longer = if self.is_up() {
            self.wait_for_down().await;
            self.debounce_delay().await; // cmk
            self.time_down = Some(Instant::now());
            LONG_PRESS_DURATION
        } else {
            // How long has the button been down so far?
            let how_long_so_far = self.time_down.map_or(Duration::from_secs(0), |time_down| {
                Instant::now() - time_down
            });

            // Calculate remaining time for a long press, ensuring it's not negative
            LONG_PRESS_DURATION
                .checked_sub(how_long_so_far)
                .unwrap_or_default()
        };
        match select(self.wait_for_release(), Timer::after(how_much_longer)).await {
            Either::First(_) => PressDuration::Short,
            Either::Second(()) => PressDuration::Long,
        }
    }

    /// Pause for a predetermined time to let the button's state become consistent.
    async fn debounce_delay(&mut self) -> &mut Self {
        Timer::after(BUTTON_DEBOUNCE_DELAY).await;
        self
    }

    /// Pause until voltage is present on the input pin.
    async fn wait_for_down(&mut self) -> &mut Self {
        self.inner.wait_for_high().await;
        self
    }

    // wait for the button to be released
    pub async fn wait_for_up(&mut self) -> &mut Self {
        self.inner.wait_for_low().await;
        self
    }

    async fn wait_for_release(&mut self) -> &mut Self {
        self.inner.wait_for_falling_edge().await;
        self
    }
}

// Instead of having API describing a short vs a long button-press vaguely using a `bool`, we define
// an `enum` to clarify what each state represents.  The compiler will compile this down to the
// very same `boolean` that we would have coded by hand.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PressDuration {
    #[default]
    Short,
    Long,
}

// Make `PressDuration` solely responsible for the distinction in `Duration` between a short and long
// button press.
impl From<Duration> for PressDuration {
    fn from(duration: Duration) -> Self {
        if duration >= LONG_PRESS_DURATION {
            Self::Long
        } else {
            Self::Short
        }
    }
}

pub const BUTTON_DEBOUNCE_DELAY: Duration = Duration::from_millis(10);
pub const LONG_PRESS_DURATION: Duration = Duration::from_millis(2000);
