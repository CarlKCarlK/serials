//! A device abstraction for buttons with debouncing and press duration detection.
//!
//! See [`Button`] for usage example.
// cmk check this now that it works connected to both ground and voltage

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
// ButtonConnection - How the button is wired
// ============================================================================

/// Describes how the button is physically wired.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, defmt::Format)]
pub enum ButtonConnection {
    /// Button connects pin to voltage (3.3V) when pressed.
    /// Uses internal pull-down resistor. Pin reads HIGH when pressed.
    ///
    /// Note: Pico 2 (RP2350) has a known silicon bug with pull-down resistors
    /// that can cause pins to stay HIGH after button release. Use ToGround instead.
    ToVoltage,

    /// Button connects pin to ground (GND) when pressed.
    /// Uses internal pull-up resistor. Pin reads LOW when pressed.
    /// Recommended for Pico 2 due to pull-down resistor bug.
    ToGround,
}

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
/// The button can be wired in two ways:
/// - [`ButtonConnection::ToVoltage`]: Button connects pin to 3.3V when pressed (uses pull-down)
/// - [`ButtonConnection::ToGround`]: Button connects pin to GND when pressed (uses pull-up)
///
/// **Important**: Pico 2 (RP2350) has a known silicon bug with pull-down resistors.
/// Use [`ButtonConnection::ToGround`] for Pico 2.
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
/// use serials::button::{Button, ButtonConnection, PressDuration};
/// # #[panic_handler]
/// # fn panic(_info: &core::panic::PanicInfo) -> ! { loop {} }
///
/// async fn example(p: embassy_rp::Peripherals) {
///     let mut button = Button::new(p.PIN_15, ButtonConnection::ToGround);
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
pub struct Button<'a> {
    input: Input<'a>,
    connection: ButtonConnection,
}

impl<'a> Button<'a> {
    /// Creates a new `Button` instance from a pin.
    ///
    /// The pin is configured based on the connection type:
    /// - [`ButtonConnection::ToVoltage`]: Uses internal pull-down (button to 3.3V)
    /// - [`ButtonConnection::ToGround`]: Uses internal pull-up (button to GND)
    #[must_use]
    pub fn new<P: embassy_rp::gpio::Pin>(pin: Peri<'a, P>, connection: ButtonConnection) -> Self {
        let pull = match connection {
            ButtonConnection::ToVoltage => Pull::Down,
            ButtonConnection::ToGround => Pull::Up,
        };
        Self {
            input: Input::new(pin, pull),
            connection,
        }
    }

    /// Returns whether the button is currently pressed.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        match self.connection {
            ButtonConnection::ToVoltage => self.input.is_high(),
            ButtonConnection::ToGround => self.input.is_low(),
        }
    }

    #[inline]
    async fn wait_for_button_up(&mut self) -> &mut Self {
        loop {
            if !self.is_pressed() {
                break;
            }
            embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
        }
        self
    }

    #[inline]
    async fn wait_for_button_down(&mut self) -> &mut Self {
        loop {
            if self.is_pressed() {
                break;
            }
            embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
        }
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
        self.wait_for_button_down().await;
        self
    }
}
