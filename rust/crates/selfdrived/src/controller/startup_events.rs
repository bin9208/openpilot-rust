use super::{effects::Effects, views::Views, Controller, Error};
use crate::state::EventType;
use openpilot_cereal::{car_capnp::car_state, log_capnp::onroad_event::EventName};
use openpilot_messaging::state::State;
impl Controller {
    pub fn update_system_ready_alert(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        if self.system_ready_alerted || self.mode.replay || self.mode.simulation {
            return Ok(());
        }
        let ready = self.initialized
            && !self.config.passive
            && (Views { state }).device()?.get_started()
            && cs.get_can_valid()
            && !cs.get_can_timeout()
            && state.all_checks(&[])?
            && !self.events.contains(EventType::NoEntry);
        if !ready {
            self.system_ready_since = None;
            return Ok(());
        }
        let since = *self.system_ready_since.get_or_insert(state.frame());
        if (state.frame() - since) as f64 * 0.01 < 0.5
            || !self.alerts.current().alert_type.is_empty()
        {
            return Ok(());
        }
        self.events.add(EventName::SystemReady, false);
        self.update_alerts(cs, state, effects)?;
        self.system_ready_alerted = true;
        Ok(())
    }
}

pub(crate) fn jetlink_fault_recent(contents: &str, now: f64) -> bool {
    contents
        .trim()
        .parse::<f64>()
        .ok()
        .is_some_and(|timestamp| (0. ..5.).contains(&(now - timestamp)))
}
#[cfg(test)]
mod tests {
    use super::jetlink_fault_recent;
    #[test]
    fn fault_timestamp_is_fresh_only_for_source_five_second_interval() {
        for (text, now, expected) in [
            ("1", 1., true),
            ("1", 5.999, true),
            ("1", 6., false),
            ("1", 0.999, false),
            ("nan", 2., false),
            ("inf", 2., false),
            ("bad", 2., false),
            ("  1.1\n", 1.1, true),
        ] {
            assert_eq!(jetlink_fault_recent(text, now), expected, "{text} at {now}");
        }
    }
}
