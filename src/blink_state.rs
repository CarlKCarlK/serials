//! BlinkState - Controls blinking behavior for LED displays

#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

mod cwf_impl {
    use super::BlinkState;
    use embassy_futures::select::{Either, select};
    use embassy_time::Timer;
    use crate::clock_4led_blinker::{BlinkerOuterNotifier, Text};
    use crate::clock_4led_display::Display;
    use crate::constants::{BLINK_OFF_DELAY_4LED, BLINK_ON_DELAY_4LED, CELL_COUNT_4LED};

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
            select(outer_notifier.wait(), Timer::after(BLINK_ON_DELAY_4LED)).await
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
        display.write_text([' '; CELL_COUNT_4LED]);
        if let Either::First((new_state, new_text)) =
            select(outer_notifier.wait(), Timer::after(BLINK_OFF_DELAY_4LED)).await
        {
            (new_state, new_text)
        } else {
            (Self::BlinkingAndOn, text)
        }
    }
    }
}

