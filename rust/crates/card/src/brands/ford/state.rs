use super::Error;
use crate::{
    core::{Message, VehicleLog},
    query::DiagnosticLevel,
    state_helpers::SpeedFilter,
};
use num_traits::ToPrimitive;
use openpilot_can::{
    dbc::{Dbc, Definitions},
    parser::Parser,
};
use openpilot_cereal::car_capnp::{car_params::TransmissionType, car_state};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default, Serialize)]
pub struct Extras {
    pub distance_button: f64,
    pub lc_button: bool,
    pub steering_pressed_cnt: u32,
    pub buttons_stock_values: Option<BTreeMap<String, f64>>,
    pub acc_tja_status_stock_values: Option<BTreeMap<String, f64>>,
    pub lkas_status_stock_values: Option<BTreeMap<String, f64>>,
}
pub struct Config {
    pub main: u8,
    pub canfd: bool,
    pub longitudinal: bool,
    pub pcm: bool,
    pub blindspots: bool,
    pub transmission: TransmissionType,
}
pub struct State {
    pub pt: Parser,
    pub camera: Parser,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    pub(super) config: Config,
    pub(super) speed: SpeedFilter,
    pub(super) defs: Definitions,
    pub(super) cluster_seen: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    speed_filter: [f64; 2],
    cluster_seen: bool,
}
pub(super) fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
pub(super) fn snapshot(
    parser: &mut Parser,
    name: &str,
    now: u64,
) -> Result<BTreeMap<String, f64>, Error> {
    let names = parser
        .dbc
        .message(name)?
        .signals
        .iter()
        .map(|s| s.name.clone())
        .collect::<Vec<_>>();
    names
        .into_iter()
        .map(|key| Ok((key.clone(), parser.signal_lazy(name, &key, now)?)))
        .collect()
}
impl State {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            speed_filter: self.speed.state(),
            cluster_seen: self.cluster_seen,
        }
    }
    pub fn new(dbc: Arc<Dbc>, config: Config, now: u64) -> Result<Self, Error> {
        let defs = dbc.definitions()?;
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            pt: Parser::new(Arc::clone(&dbc), config.main, now),
            camera: Parser::new(dbc, config.main + 2, now),
            out,
            extras: Extras::default(),
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            config,
            speed: SpeedFilter::new()?,
            defs,
            cluster_seen: false,
        })
    }
    pub(super) fn signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.pt.signal_lazy(name, key, now)?)
    }
    pub(super) fn camera_signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.camera.signal_lazy(name, key, now)?)
    }
    pub(super) fn drain_logs(&mut self) {
        for parser in [&mut self.pt, &mut self.camera] {
            self.logs.extend(
                std::mem::take(&mut parser.diagnostics)
                    .into_iter()
                    .map(|d| VehicleLog {
                        level: DiagnosticLevel::Warning,
                        message: d.message,
                    }),
            );
        }
    }
}
