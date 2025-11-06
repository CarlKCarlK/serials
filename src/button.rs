//! Button virtual device - handles button press detection and debouncing

use defmt::info;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::Input;
use embassy_time::{Duration, Timer};

// ============================================================================
// Constants
// ============================================================================

/// Debounce delay for the button.
pub const BUTTON_DEBOUNCE_DELAY: Duration = Duration::from_millis(10);

/// Duration representing a long button press.
pub const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);

// ============================================================================
// PressDuration - Button press type
// ============================================================================

/// Represents the duration of a button press.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, defmt::Format)]
pub enum PressDuration {
    Short,
    Long,
}

// ============================================================================
// Button Virtual Device
// ============================================================================

/// A button abstraction backed by an Embassy input pin.
pub struct Button<'a>(Input<'a>);

impl<'a> Button<'a> {
    /// Creates a new `Button` instance.
    #[must_use]
    pub const fn new(button: Input<'a>) -> Self {
        Self(button)
    }

    #[inline]
    async fn wait_for_button_up(&mut self) -> &mut Self {
        self.0.wait_for_low().await;
        self
    }

    #[inline]
    async fn wait_for_button_down(&mut self) -> &mut Self {
        self.0.wait_for_high().await;
        self
    }

    /// Measures the duration of a button press.
    ///
    /// This method does not wait for the button to be released. It only waits
    /// as long as necessary to determine whether the press was "short" or "long".
    pub async fn press_duration(&mut self) -> PressDuration {
        self.wait_for_button_up().await;
        Timer::after(BUTTON_DEBOUNCE_DELAY).await;
        self.wait_for_button_down().await;
        Timer::after(BUTTON_DEBOUNCE_DELAY).await;
        let press_duration =
            match select(self.wait_for_button_up(), Timer::after(LONG_PRESS_DURATION)).await {
                Either::First(_) => PressDuration::Short,
                Either::Second(()) => PressDuration::Long,
            };
        info!("Press duration: {:?}", press_duration);
        press_duration
    }

    /// Waits for the button to be pressed.
    #[inline]
    pub async fn wait_for_press(&mut self) -> &mut Self {
        self.0.wait_for_rising_edge().await;
        self
    }
}
