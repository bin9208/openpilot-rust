use super::{projection::Data, Error, Input};
use crate::models::{ClusterUiState, CruiseDisplayState, EnergyGaugeLabel, SteeringOutputKind};
use num_traits::ToPrimitive;
use openpilot_ui_application::onroad::hud::presentation;

#[derive(Default)]
struct Steering {
    actual: Option<f64>,
    normalized: Option<f64>,
    kind: Option<SteeringOutputKind>,
}

impl Input {
    pub(super) fn hud(&self, state: &mut ClusterUiState) -> Result<(), Error> {
        state.onroad = self.onroad_state()?.unwrap_or(false);
        let car = self.data("carState")?;
        let controls = self.data("controlsState")?;
        let output = self.data("carOutput")?.field("actuatorsOutput");
        state.fuel_gauge = car
            .field("fuelGauge")
            .float()
            .filter(|value| *value > 0.0 && *value <= 1.0);
        state.urea_gauge = car
            .field("ureaGauge")
            .float()
            .filter(|value| *value > 0.0 && *value <= 1.0);
        state.energy_gauge_label = if car.flag("charging", false) {
            EnergyGaugeLabel::Battery
        } else {
            self.energy_label
        };
        let steering = self.steering(car, controls, output)?;
        state.steering_output = steering.actual;
        state.steering_output_normalized = steering.normalized;
        state.steering_output_kind = steering.kind;
        let man = self.data("carrotMan")?;
        let navi = self.subscriber.state.topic("carrotNavi")?;
        let connected = navi.alive && navi.valid && Data(navi.data()?).flag("connected", false);
        let media_connected = state
            .navi_dashboard
            .as_ref()
            .is_some_and(|dashboard| dashboard.connected);
        state.external_nav_active = presentation::external_connected(
            &man.field("remote").text().unwrap_or_default(),
            connected || media_connected,
        );
        state.navi_owner = man.field("naviOwner").text().unwrap_or_default();
        state.navi_lifecycle = man.field("naviLifecycle").text().unwrap_or_default();
        state.vehicle_navi_available = man.flag("vehicleNaviAvailable", false);
        let plan = self.subscriber.state.topic("longitudinalPlan")?;
        state.driving_mode = state
            .driving_mode
            .filter(|mode| (1..=4).contains(mode) && plan.alive && plan.valid);
        state.cruise_override_kph = None;
        state.cruise_override_label = None;
        state.cruise_override_color_mode = 0;
        if let Some(cruise) = state
            .cruise_kph
            .filter(|_| state.cruise_display_state != CruiseDisplayState::Off)
        {
            let source = man.field("desiredSource").text().unwrap_or_default();
            let provider = man.field("decelProvider").text().unwrap_or_default();
            let result = presentation::SetSpeed {
                cruise_target: Data(plan.data()?).field("cruiseTarget").float(),
                desired_speed: man.field("desiredSpeed").float(),
                source: &source,
                provider: &provider,
                set_speed_kph: cruise.to_f64().ok_or(Error::Clock)?,
                max_label: "",
            }
            .compute();
            if result.active {
                state.cruise_override_kph = Some(result.speed_kph);
                state.cruise_override_label = Some(result.label);
                state.cruise_override_color_mode = i64::from(result.speed_color_mode);
            }
        }
        Ok(())
    }
    fn steering(
        &self,
        car: Data<'_>,
        controls: Data<'_>,
        output: Data<'_>,
    ) -> Result<Steering, Error> {
        let kind = controls
            .field("lateralControlState")
            .variant()
            .unwrap_or_default();
        if kind == "angleState" {
            let normalized = if self.data("carControl")?.flag("latActive", false) {
                let speed = car.number("vEgo", 0.0).max(0.0);
                let curvature = controls.number("curvature", 0.0);
                let desired = controls
                    .field("desiredCurvature")
                    .float()
                    .filter(|value| *value != 0.0)
                    .unwrap_or(curvature);
                let roll = self.data("liveParameters")?.number("roll", 0.0);
                let compensation = roll * 9.80665 * ((speed - 5.0) / 10.0).clamp(0.0, 1.0);
                ((desired * speed * speed - compensation) / self.max_lateral_accel).clamp(-1.0, 1.0)
            } else {
                0.0
            };
            return Ok(Steering {
                actual: Some(normalized * 100.0),
                normalized: Some(normalized),
                kind: Some(SteeringOutputKind::Angle),
            });
        }
        if matches!(kind.as_str(), "torqueState" | "pidState") {
            let normalized = output
                .field("torque")
                .float()
                .map(|value| value.clamp(-1.0, 1.0));
            let actual = output
                .field("torqueOutputCan")
                .float()
                .filter(|value| value.abs() <= 999.0)
                .or_else(|| normalized.map(|value| value * 100.0));
            return Ok(Steering {
                actual,
                normalized,
                kind: Some(SteeringOutputKind::Torque),
            });
        }
        Ok(Steering::default())
    }
}
