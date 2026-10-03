use super::{
    config::Family,
    state::{Bus, State},
    Error,
};
use openpilot_cereal::car_capnp::car_state::{self, button_event::Type as Button};
impl State {
    pub(super) fn buttons(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
        latching: bool,
    ) -> Result<(), Error> {
        let (message, keys) = match self.config.family {
            Family::Pq => (
                "GRA_Neu",
                [
                    "GRA_Neu_Setzen",
                    "GRA_Recall",
                    "GRA_Up_kurz",
                    "GRA_Down_kurz",
                    "GRA_Abbrechen",
                    "GRA_Zeitluecke",
                ],
            ),
            Family::Mqb => (
                "GRA_ACC_01",
                [
                    "GRA_Tip_Setzen",
                    "GRA_Tip_Wiederaufnahme",
                    "GRA_Tip_Hoch",
                    "GRA_Tip_Runter",
                    "GRA_Abbrechen",
                    "GRA_Verstellung_Zeitluecke",
                ],
            ),
            Family::Meb => (
                "GRA_ACC_01",
                [
                    "GRA_Tip_Setzen",
                    "GRA_Tip_Wiederaufnahme",
                    "GRA_Tip_Hoch",
                    "GRA_Tip_Runter",
                    if latching {
                        "GRA_Abbrechen"
                    } else {
                        "GRA_Hauptschalter"
                    },
                    "GRA_Verstellung_Zeitluecke",
                ],
            ),
        };
        let order = match self.config.family {
            Family::Meb => [0, 1, 2, 3, 5, 4],
            Family::Pq | Family::Mqb => [0, 1, 2, 3, 4, 5],
        };
        let names = [
            "setCruise",
            "resumeCruise",
            "accelCruise",
            "decelCruise",
            "cancel",
            "gapAdjustCruise",
        ];
        let types = [
            Button::SetCruise,
            Button::ResumeCruise,
            Button::AccelCruise,
            Button::DecelCruise,
            Button::Cancel,
            Button::GapAdjustCruise,
        ];
        let mut events = Vec::new();
        for index in order {
            let value = self.signal(Bus::Pt, (message, keys[index]), now)?;
            let current = value
                == if matches!(self.config.family, Family::Meb) && index == 5 {
                    3.
                } else {
                    1.
                };
            let previous = self
                .extras
                .button_states
                .get_mut(names[index])
                .ok_or_else(|| Error::Signal(names[index].into()))?;
            if *previous != current {
                events.push((types[index], current));
            }
            *previous = current;
        }
        let mut list = ret
            .reborrow()
            .init_button_events(u32::try_from(events.len()).map_err(|_| Error::Numeric)?);
        for (index, (kind, pressed)) in events.into_iter().enumerate() {
            let mut event = list
                .reborrow()
                .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
            event.set_type(kind);
            event.set_pressed(pressed);
        }
        Ok(())
    }
    pub(super) fn blinkers(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        time: u32,
        stalk: [bool; 2],
    ) {
        let [left, right] = stalk;
        if left {
            self.extras.right_blinker_cnt = 0;
            if !self.extras.left_blinker_prev {
                self.extras.left_blinker_cnt = time;
            }
        }
        if right {
            self.extras.left_blinker_cnt = 0;
            if !self.extras.right_blinker_prev {
                self.extras.right_blinker_cnt = time;
            }
        }
        self.extras.left_blinker_cnt = self.extras.left_blinker_cnt.saturating_sub(1);
        self.extras.right_blinker_cnt = self.extras.right_blinker_cnt.saturating_sub(1);
        self.extras.left_blinker_prev = left;
        self.extras.right_blinker_prev = right;
        ret.set_left_blinker(left || self.extras.left_blinker_cnt > 0);
        ret.set_right_blinker(right || self.extras.right_blinker_cnt > 0);
    }
    pub(super) fn hca_faults(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        status: Option<&str>,
        drive: bool,
    ) {
        self.extras.eps_init_complete = self.extras.eps_init_complete
            || matches!(status, Some("DISABLED" | "READY" | "ACTIVE"))
            || self.extras.frame > 600;
        let permanent = match self.config.family {
            Family::Meb => false,
            Family::Mqb | Family::Pq => drive && status == Some("DISABLED"),
        } || self.extras.eps_init_complete && status == Some("FAULT");
        ret.set_steer_fault_permanent(permanent);
        ret.set_steer_fault_temporary(
            drive && matches!(status, Some("REJECTED" | "PREEMPTED"))
                || !self.extras.eps_init_complete,
        );
    }
}
