//! A device abstraction for buttons with debouncing and press duration detection.
//!
//! See [`Button`] for usage example.

use defmt::info;
use embassy_futures::select::{Either, select};
use embassy_rp::Peri;
use embassy_rp::gpio::{Input, Pull};
use embassy_time::{Duration, Timer};

// ============================================================================
// Constants
// ============================================================================

/// Debounce delay for the button.
const BUTTON_DEBOUNCE_DELAY: Duration = Duration::from_millis(10);

/// Duration representing a long button press.
const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);

// ============================================================================
// PressDuration - Button press type
// ============================================================================

/// Duration of a button press (short or long).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, defmt::Format)]
pub enum PressDuration {
    Short,
    Long,
}

// ============================================================================
// Button Virtual Device
// ============================================================================

/// A device abstraction for a button with debouncing and press duration detection.
///
/// # Hardware Requirements
///
/// The button should be wired to connect the pin to 3.3V when pressed. The pin is
/// configured with an internal pull-down resistor, so no external resistor is needed.
///
/// # Usage
///
/// The [`wait_for_press_duration()`](Self::wait_for_press_duration) method returns as soon as it determines
/// whether the press is short or long, without waiting for the button to be released.
/// This allows for responsive UI feedback.
///
/// If you only need to detect when the button is pressed without measuring duration,
/// use [`wait_for_press()`](Self::wait_for_press) instead.
///
/// # Example
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
///
/// use serials::button::{Button, PressDuration};
/// # #[panic_handler]
/// # fn panic(_info: &core::panic::PanicInfo) -> ! { loop {} }
///
/// async fn example(p: embassy_rp::Peripherals) {
///     let mut button = Button::new(p.PIN_15);
///
///     // Measure press durations in a loop
///     loop {
///         match button.wait_for_press_duration().await {
///             PressDuration::Short => {
///                 // Handle short press
///             }
///             PressDuration::Long => {
///                 // Handle long press (fires before button is released)
///             }
///         }
///     }
/// }
/// ```
pub struct Button<'a>(Input<'a>);

impl<'a> Button<'a> {
    /// Creates a new `Button` instance from a pin.
    ///
    /// The pin is configured with an internal pull-down resistor.
    #[must_use]
    pub fn new<P: embassy_rp::gpio::Pin>(pin: Peri<'a, P>) -> Self {
        Self(Input::new(pin, Pull::Down))
    }

    /// Returns whether the button is currently pressed.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.0.is_high()
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
    ///
    /// See also: [`wait_for_press()`](Self::wait_for_press) for simple press detection.
    pub async fn wait_for_press_duration(&mut self) -> PressDuration {
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
    ///
    /// This method returns immediately when the button press is detected,
    /// without waiting for release or measuring duration.
    ///
    /// Use this when you only need to detect button presses. If you need to
    /// distinguish between short and long presses, use [`wait_for_press_duration()`](Self::wait_for_press_duration) instead.
    #[inline]
    pub async fn wait_for_press(&mut self) -> &mut Self {
        self.0.wait_for_rising_edge().await;
        self
    }
}
