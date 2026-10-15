//! Pinned Toyota PCM set-speed observation; never generates button commands.
use openpilot_cereal::car_capnp::car_state;
use serde::Serialize;
#[derive(Serialize)]
pub struct ToyotaPcmSetSpeedSync {
    pub max_speed_kph: f64,
    _observed_speed_kph: Option<f64>,
    _was_enabled: Option<bool>,
}
impl Default for ToyotaPcmSetSpeedSync {
    fn default() -> Self {
        Self {
            max_speed_kph: 145.,
            _observed_speed_kph: None,
            _was_enabled: None,
        }
    }
}
fn round_three(value: f64) -> f64 {
    // Decimal formatting rounds the binary input directly (including ties to even),
    // preserving Python round(x,3); scaling then rounding can change half-way cases.
    format!("{value:.3}").parse().expect("finite decimal")
}
impl ToyotaPcmSetSpeedSync {
    fn reset(&mut self) {
        self._observed_speed_kph = None;
        self._was_enabled = None;
    }
    pub fn update(
        &mut self,
        eligible: bool,
        cs: car_state::Reader<'_>,
        local_changed: bool,
    ) -> Result<Option<[f64; 2]>, capnp::Error> {
        if !eligible || !cs.get_can_valid() {
            self.reset();
            return Ok(None);
        }
        let cruise = cs.get_cruise_state()?;
        if !cruise.get_available() {
            self.reset();
            return Ok(None);
        }
        let speed = f64::from(cruise.get_speed()) * 3.6;
        if !speed.is_finite() {
            self.reset();
            return Ok(None);
        }
        let unset = speed == 0.;
        let speed = round_three(speed);
        let enabled = cruise.get_enabled();
        if unset && !enabled {
            self._observed_speed_kph = None;
            self._was_enabled = Some(false);
            return Ok(None);
        }
        if !(0. < speed && speed <= self.max_speed_kph) {
            self.reset();
            return Ok(None);
        }
        let resumed = enabled && self._was_enabled == Some(false);
        let changed = self
            ._observed_speed_kph
            .is_some_and(|old| (speed - old).abs() >= 0.5);
        self._observed_speed_kph = Some(speed);
        self._was_enabled = Some(enabled);
        if !enabled || local_changed || !(resumed || changed) {
            return Ok(None);
        }
        let cluster = f64::from(cruise.get_speed_cluster()) * 3.6;
        let cluster = if !cluster.is_finite() || cluster <= 0. {
            speed
        } else {
            let rounded = round_three(cluster);
            if 0. < rounded && rounded <= self.max_speed_kph {
                rounded
            } else {
                speed
            }
        };
        Ok(Some([speed, cluster]))
    }
}
