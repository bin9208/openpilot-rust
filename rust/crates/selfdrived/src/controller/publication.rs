use super::{
    effects::{Effects, Publications},
    Controller, Error,
};
use crate::state::{EventType, State as ControlState};
use openpilot_cereal::log_capnp::{event, selfdrive_state::OpenpilotState};
use openpilot_messaging::state::State;

impl Controller {
    pub fn publish(
        &mut self,
        state: &State,
        effects: &mut impl Effects,
        publications: &mut impl Publications,
    ) -> Result<(), Error> {
        self.refresh_runtime_settings()?;
        let mut message = capnp::message::Builder::new_default();
        let mut root: event::Builder<'_> = message.init_root();
        root.set_log_mono_time(effects.timestamp()?);
        root.set_valid(true);
        let mut output = root.init_selfdrive_state();
        output.set_enabled(self.enabled);
        output.set_active(self.active);
        output.set_state(match self.state_machine.state {
            ControlState::Disabled => OpenpilotState::Disabled,
            ControlState::PreEnabled => OpenpilotState::PreEnabled,
            ControlState::Enabled => OpenpilotState::Enabled,
            ControlState::SoftDisabling => OpenpilotState::SoftDisabling,
            ControlState::Overriding => OpenpilotState::Overriding,
        });
        output.set_engageable(!self.events.contains(EventType::NoEntry));
        output.set_experimental_mode(self.experimental_mode);
        output.set_personality(self.personality.wire()?);
        let alert = self.alerts.current();
        output.set_alert_text1(alert.alert_text_1.as_str());
        output.set_alert_text2(alert.wire_text_2()?);
        output.set_alert_size(alert.alert_size);
        output.set_alert_status(alert.alert_status);
        output.set_alert_type(alert.alert_type.as_str());
        output.set_alert_sound(alert.audible_alert);
        output.set_alert_hud_visual(alert.visual_alert);
        output.set_distance_traveled(self.distance_traveled as f32);
        publications.send(
            "selfdriveState",
            &capnp::serialize::write_message_to_words(&message),
        )?;
        if state.frame() % 100 == 0 || self.events.names() != self.events_previous {
            let mut message = capnp::message::Builder::new_default();
            let mut root: event::Builder<'_> = message.init_root();
            root.set_log_mono_time(effects.timestamp()?);
            root.set_valid(true);
            let mut events = root.init_onroad_events(
                u32::try_from(self.events.names().len())
                    .map_err(|_| Error::Contract("too many events"))?,
            );
            for (index, event) in self.events.names().iter().enumerate() {
                let mut output = events
                    .reborrow()
                    .get(u32::try_from(index).map_err(|_| Error::Contract("too many events"))?);
                output.set_name(*event);
                for category in self.events.categories(*event) {
                    match category.category {
                        EventType::Enable => output.set_enable(true),
                        EventType::PreEnable => output.set_pre_enable(true),
                        EventType::OverrideLateral => output.set_override_lateral(true),
                        EventType::OverrideLongitudinal => output.set_override_longitudinal(true),
                        EventType::NoEntry => output.set_no_entry(true),
                        EventType::Warning => output.set_warning(true),
                        EventType::UserDisable => output.set_user_disable(true),
                        EventType::SoftDisable => output.set_soft_disable(true),
                        EventType::ImmediateDisable => output.set_immediate_disable(true),
                        EventType::Permanent => output.set_permanent(true),
                    }
                }
            }
            publications.send(
                "onroadEvents",
                &capnp::serialize::write_message_to_words(&message),
            )?;
        }
        self.events_previous = self.events.names().to_vec();
        Ok(())
    }
}
