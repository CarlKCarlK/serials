use embassy_futures::select::{select, Either};
use embassy_time::Timer;

use crate::{
    // cmk need to update Clock to match Blinker
    blinker::{BlinkerOuterNotifier, Text},
    Display,
    BLINK_OFF_DELAY,
    BLINK_ON_DELAY,
    CELL_COUNT,
};

#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

impl BlinkState {
    #[inline]
    pub async fn run_and_next(
        self,
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        match self {
            Self::Solid => self.run_and_next_solid(outer_notifier, display, text).await,
            Self::BlinkingAndOn => {
                self.run_and_next_blinking_and_on(outer_notifier, display, text)
                    .await
            }
            Self::BlinkingButOff => {
                self.run_and_next_blinking_but_off(outer_notifier, display, text)
                    .await
            }
        }
    }

    async fn run_and_next_solid(
        self,
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        display.write_text(text);
        outer_notifier.wait().await
    }

    async fn run_and_next_blinking_and_on(
        self,
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        display.write_text(text);
        if let Either::First((new_blink_state, new_text)) =
            select(outer_notifier.wait(), Timer::after(BLINK_ON_DELAY)).await
        {
            (new_blink_state, new_text)
        } else {
            (Self::BlinkingButOff, text)
        }
    }

    async fn run_and_next_blinking_but_off(
        self,
        outer_notifier: &'static BlinkerOuterNotifier,
        display: &Display<'_>,
        text: Text,
    ) -> (Self, Text) {
        display.write_text([' '; CELL_COUNT]);
        if let Either::First((new_blink_state, new_text)) =
            select(outer_notifier.wait(), Timer::after(BLINK_OFF_DELAY)).await
        {
            (new_blink_state, new_text)
        } else {
            (Self::BlinkingAndOn, text)
        }
    }
}
