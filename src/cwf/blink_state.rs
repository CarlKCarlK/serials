use embassy_futures::select::{select, Either};
use embassy_time::Timer;

use crate::cwf::blinker::{BlinkerOuterNotifier, Text};
use crate::cwf::display::Display;
use crate::cwf::shared_constants::{BLINK_OFF_DELAY, BLINK_ON_DELAY, CELL_COUNT};

#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

impl BlinkState {
    #[inline]
    pub async fn execute(
        self,
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        match self {
            Self::Solid => Self::execute_solid(outer_notifier, display, text).await,
            Self::BlinkingAndOn => {
                Self::execute_blinking_and_on(outer_notifier, display, text).await
            }
            Self::BlinkingButOff => {
                Self::execute_blinking_but_off(outer_notifier, display, text).await
            }
        }
    }

    async fn execute_solid(
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        display.write_text(text);
        outer_notifier.wait().await
    }

    async fn execute_blinking_and_on(
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        display.write_text(text);
        if let Either::First((new_state, new_text)) =
            select(outer_notifier.wait(), Timer::after(BLINK_ON_DELAY)).await
        {
            (new_state, new_text)
        } else {
            (Self::BlinkingButOff, text)
        }
    }

    async fn execute_blinking_but_off(
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        display.write_text([' '; CELL_COUNT]);
        if let Either::First((new_state, new_text)) =
            select(outer_notifier.wait(), Timer::after(BLINK_OFF_DELAY)).await
        {
            (new_state, new_text)
        } else {
            (Self::BlinkingAndOn, text)
        }
    }
}
