use super::{projection::Data, Input};
use crate::models::EnergyGaugeLabel;
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::{car_capnp::car_params, log_capnp::event};
use openpilot_params::Params;
use openpilot_ui_application::params::numeric;

pub(super) struct Settings {
    pub persistent: Params,
    pub memory: Option<Params>,
    pub egpu: bool,
    next_egpu: f64,
}
impl Settings {
    pub fn new(persistent: Params, memory: Option<Params>) -> Self {
        Self {
            persistent,
            memory,
            egpu: false,
            next_egpu: 0.0,
        }
    }
    pub fn refresh_egpu(&mut self, now: f64) {
        if now >= self.next_egpu {
            self.egpu = self.persistent.get_bool("UsbGpuActive").unwrap_or(false);
            self.next_egpu = now + 1.0;
        }
    }
    pub fn float(&self, key: &str, scale: f64) -> Option<f64> {
        let bytes = self.persistent.get(key).ok().flatten().unwrap_or_default();
        let value = numeric::float(&bytes).unwrap_or_else(|| fatal_numeric(key)) * scale;
        value.is_finite().then_some(value)
    }
    pub fn integer(&self, key: &str, fallback: i64) -> i64 {
        let bytes = self.persistent.get(key).ok().flatten().unwrap_or_default();
        i64::from(numeric::integer(&bytes).unwrap_or_else(|| fatal_numeric(key))).max(fallback)
    }
    pub fn event<T>(
        &self,
        key: &str,
        service: &str,
        project: impl FnOnce(Data<'_>) -> Option<T>,
    ) -> Option<T> {
        let bytes = self.persistent.get(key).ok().flatten()?;
        let message = serialize::read_message(&mut &bytes[..], ReaderOptions::new()).ok()?;
        let event = message.get_root::<event::Reader<'_>>().ok()?;
        let capnp::dynamic_value::Reader::Struct(event) = event.into() else {
            return None;
        };
        let field = event.which().ok().flatten()?;
        if field.get_proto().get_name().ok()?.to_str().ok()? != service {
            return None;
        }
        project(Data(event.get(field).ok()?))
    }
}
fn fatal_numeric(key: &str) -> ! {
    eprintln!("[cluster] fatal malformed numeric Params value: {key}");
    std::process::abort()
}
impl Input {
    pub fn attach_params(&mut self, params: Params, memory: Option<Params>) {
        if let Some(bytes) = params.get("CarParams").ok().flatten() {
            if let Ok(message) = serialize::read_message(&mut &bytes[..], ReaderOptions::new()) {
                if let Ok(cp) = message.get_root::<car_params::Reader<'_>>() {
                    let data = Data(cp.into());
                    self.projection.apply_data("carParams", data.0, 0.0, true);
                    self.energy_label = if data.field("brand").text().as_deref() == Some("hyundai")
                        && data
                            .field("flags")
                            .uint()
                            .is_some_and(|flags| flags & (1 << 11) != 0)
                    {
                        EnergyGaugeLabel::Battery
                    } else {
                        EnergyGaugeLabel::Fuel
                    };
                    self.max_lateral_accel = data
                        .field("maxLateralAccel")
                        .float()
                        .filter(|value| *value > 0.0)
                        .unwrap_or(3.0);
                }
            }
        }
        if let Some(bytes) = params.get("CalibrationParams").ok().flatten() {
            if let Ok(message) = serialize::read_message(&mut &bytes[..], ReaderOptions::new()) {
                if let Ok(event) = message.get_root::<event::Reader<'_>>() {
                    if let Ok(event::LiveCalibration(Ok(calibration))) = event.which() {
                        self.projection.apply_data(
                            "liveCalibration",
                            calibration.into(),
                            0.0,
                            event.get_valid(),
                        );
                    }
                }
            }
        }
        self.params = Some(Settings::new(params, memory));
    }
}
