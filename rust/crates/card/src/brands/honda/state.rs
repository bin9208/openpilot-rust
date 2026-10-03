use super::{config::Config, Error, NIDEC_ALT_SCM_MESSAGES};
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

pub type Values = BTreeMap<String, f64>;
#[derive(Serialize)]
#[serde(untagged)]
pub enum Stock {
    Inactive(bool),
    Values(Values),
}
impl Stock {
    pub(super) fn values(&self, name: &'static str) -> Result<&Values, Error> {
        match self {
            Self::Values(values) => Ok(values),
            Self::Inactive(_) => Err(Error::Stock(name)),
        }
    }
}
#[derive(Default, Serialize)]
pub struct Extras {
    pub brake_switch_prev: bool,
    pub brake_switch_active: bool,
    pub cruise_setting: f64,
    pub cruise_buttons: f64,
    pub v_cruise_pcm_prev: f64,
    pub dash_speed_seen: bool,
    pub acc_hud: Option<Stock>,
    pub lkas_hud: Option<Stock>,
    pub stock_brake: Option<Values>,
    pub left_blinker_cnt: u32,
    pub right_blinker_cnt: u32,
    pub left_blinker_prev: bool,
    pub right_blinker_prev: bool,
}
pub struct State {
    pub pt: Parser,
    pub camera: Parser,
    pub body: Option<Parser>,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    pub(super) config: Config,
    pub(super) speed: SpeedFilter,
    pub(super) defs: Definitions,
    pub(super) cluster_seen: bool,
    pub(super) gearbox: &'static str,
    pub(super) main_message: &'static str,
}
#[derive(Serialize)]
pub struct Snapshot {
    speed_filter: [f64; 2],
    cluster_seen: bool,
}
pub(super) fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
pub(super) fn copy(parser: &mut Parser, name: &str, now: u64) -> Result<Values, Error> {
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
    pub fn new(
        dbc: Arc<Dbc>,
        body: Option<Arc<Dbc>>,
        config: Config,
        now: u64,
    ) -> Result<Self, Error> {
        let gearbox = match config.transmission {
            TransmissionType::Cvt if config.candidate == "HONDA_ACCORD" => "GEARBOX_15T",
            TransmissionType::Cvt if config.candidate == "HONDA_CIVIC_2022" => "GEARBOX_ALT",
            TransmissionType::Manual => "GEARBOX_ALT_2",
            TransmissionType::Unknown
            | TransmissionType::Automatic
            | TransmissionType::Cvt
            | TransmissionType::Direct => "GEARBOX",
        };
        let main_message = if config.static_flags & NIDEC_ALT_SCM_MESSAGES != 0 {
            "SCM_BUTTONS"
        } else {
            "SCM_FEEDBACK"
        };
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        let defs = dbc.definitions()?;
        if config.transmission != TransmissionType::Manual {
            let address = dbc.message(gearbox)?.address;
            if !defs
                .get(&address)
                .is_some_and(|d| d.contains_key("GEAR_SHIFTER"))
            {
                return Err(Error::Signal(format!("{gearbox}.GEAR_SHIFTER definitions")));
            }
        }
        let steer_address = dbc.message("STEER_STATUS")?.address;
        if !defs
            .get(&steer_address)
            .is_some_and(|d| d.contains_key("STEER_STATUS"))
        {
            return Err(Error::Signal(
                "STEER_STATUS.STEER_STATUS definitions".into(),
            ));
        }
        Ok(Self {
            defs,
            pt: Parser::new(Arc::clone(&dbc), config.bus.pt, now),
            camera: Parser::new(dbc, config.bus.camera, now),
            body: body.map(|d| Parser::new(d, config.bus.radar, now)),
            out,
            extras: Extras::default(),
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            config,
            speed: SpeedFilter::new()?,
            cluster_seen: false,
            gearbox,
            main_message,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            speed_filter: self.speed.state(),
            cluster_seen: self.cluster_seen,
        }
    }
    pub(super) fn signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.pt.signal_lazy(name, key, now)?)
    }
    pub(super) fn camera_signal(&mut self, name: &str, key: &str, now: u64) -> Result<f64, Error> {
        Ok(self.camera.signal_lazy(name, key, now)?)
    }
    pub(super) fn drain_logs(&mut self) {
        let mut parsers = vec![&mut self.pt, &mut self.camera];
        if let Some(body) = &mut self.body {
            parsers.push(body);
        }
        for parser in parsers {
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
