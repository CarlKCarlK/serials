use embassy_futures::select::{select, Either};
use embassy_time::Timer;

use crate::{blinker::NotifierInner, Display, BLINK_OFF_DELAY, BLINK_ON_DELAY, CELL_COUNT};

#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

impl BlinkState {
    pub async fn run_and_next(
        self,
        notifier: &'static NotifierInner,
        display: &Display<'_>,
        chars: [char; CELL_COUNT],
    ) -> (Self, [char; CELL_COUNT]) {
        match self {
            Self::Solid => self.run_and_next_solid(notifier, display, chars).await,
            Self::BlinkingAndOn => {
                self.run_and_next_blinking_and_on(notifier, display, chars)
                    .await
            }
            Self::BlinkingButOff => {
                self.run_and_next_blinking_but_off(notifier, display, chars)
                    .await
            }
        }
    }

    async fn run_and_next_solid(
        self,
        notifier: &'static NotifierInner,
        display: &Display<'_>,
        chars: [char; CELL_COUNT],
    ) -> (Self, [char; CELL_COUNT]) {
        display.write_chars(chars);
        notifier.wait().await
    }

    async fn run_and_next_blinking_and_on(
        self,
        notifier: &'static NotifierInner,
        display: &Display<'_>,
        chars: [char; CELL_COUNT],
    ) -> (Self, [char; CELL_COUNT]) {
        display.write_chars(chars);
        if let Either::First((new_blink_state, new_chars)) =
            select(notifier.wait(), Timer::after(BLINK_ON_DELAY)).await
        {
            (new_blink_state, new_chars)
        } else {
            (Self::BlinkingButOff, chars)
        }
    }

    async fn run_and_next_blinking_but_off(
        self,
        notifier: &'static NotifierInner,
        display: &Display<'_>,
        chars: [char; CELL_COUNT],
    ) -> (Self, [char; CELL_COUNT]) {
        display.write_chars([' '; CELL_COUNT]);
        if let Either::First((new_blink_state, new_chars)) =
            select(notifier.wait(), Timer::after(BLINK_OFF_DELAY)).await
        {
            (new_blink_state, new_chars)
        } else {
            (Self::BlinkingAndOn, chars)
        }
    }
}
