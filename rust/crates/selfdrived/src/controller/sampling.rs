use super::{diagnostics, effects::Effects, views, Controller, Error};
use openpilot_cereal::car_capnp::car_state;
use openpilot_messaging::state::State;
impl Controller {
    pub fn data_sample(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &mut State,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        if !self.initialized {
            let all_valid = cs.get_can_valid() && state.all_checks(&[])?;
            let timed_out = state.frame() as f64 * 0.01 > 6.0;
            if all_valid || timed_out || (self.mode.simulation && !self.mode.replay) {
                let available = effects.available_streams()?;
                if !available.contains(&openpilot_msgq::VisionStream::Road) {
                    state.append_shared_ignore_alive_valid("roadCameraState")?;
                }
                if self.use_wide_camera
                    && !available.contains(&openpilot_msgq::VisionStream::WideRoad)
                {
                    state.append_shared_ignore_alive_valid("wideRoadCameraState")?;
                }
                let views = views::Views { state };
                if self.mode.replay
                    && views
                        .pandas()?
                        .iter()
                        .any(|panda| panda.get_controls_allowed())
                {
                    self.state_machine.state = crate::state::State::Enabled;
                }
                self.initialized = true;
                let mut fields = openpilot_logging::Fields::new();
                fields.insert(
                    "dt".into(),
                    openpilot_logging::Value::Float(state.frame() as f64 * 0.01),
                );
                fields.insert("timeout".into(), openpilot_logging::Value::Bool(timed_out));
                fields.insert(
                    "canValid".into(),
                    openpilot_logging::Value::Bool(cs.get_can_valid()),
                );
                fields.extend(
                    diagnostics::issues(state)
                        .fields()
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
                fields.insert("error".into(), openpilot_logging::Value::Bool(true));
                effects.event("selfdrived.initialized", fields)?;
            }
        }
        if !self.enabled {
            self.mismatch_counter = 0;
        }
        if self.enabled {
            for panda in (views::Views { state }).pandas()?.iter() {
                if !matches!(
                    panda.get_safety_model()?,
                    openpilot_cereal::car_capnp::car_params::SafetyModel::Silent
                        | openpilot_cereal::car_capnp::car_params::SafetyModel::NoOutput
                ) && !panda.get_controls_allowed()
                {
                    self.mismatch_counter += 1;
                    break;
                }
            }
        }
        Ok(())
    }
}
