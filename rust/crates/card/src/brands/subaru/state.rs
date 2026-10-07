use super::{Error, GLOBAL_GEN2, HYBRID};
use crate::{
    core::{Message, VehicleLog},
    query::DiagnosticLevel,
    state_helpers::SpeedFilter,
};
use openpilot_can::{
    dbc::{Dbc, Definitions},
    parser::Parser,
    Packet,
};
use openpilot_cereal::car_capnp::{car_params, car_state};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

pub type Values = BTreeMap<String, f64>;
#[derive(Default, Serialize)]
pub struct Extras {
    pub cruise_button: Option<f64>,
    pub ready: Option<bool>,
    pub es_distance_msg: Option<Values>,
    pub es_dashstatus_msg: Option<Values>,
    pub es_lkas_state_msg: Option<Values>,
    pub es_brake_msg: Option<Values>,
    pub es_status_msg: Option<Values>,
    pub cruise_control_msg: Option<Values>,
    pub es_infotainment_msg: Option<Values>,
    pub left_blinker_cnt: u32,
    pub right_blinker_cnt: u32,
    pub angle_previous_value: f64,
    pub angle_rate: f64,
}
pub struct State {
    pub pt: Parser,
    pub camera: Parser,
    pub alt: Parser,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    pub(super) flags: u32,
    pub(super) longitudinal: bool,
    pub(super) pcm: bool,
    pub(super) bsm: bool,
    pub(super) factor: f64,
    pub(super) defs: Definitions,
    pub(super) speed: SpeedFilter,
    pub(super) cluster_seen: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    speed_filter: [f64; 2],
    cluster_seen: bool,
}
#[derive(Clone, Copy)]
pub(super) enum Bus {
    Pt,
    Cam,
    Alt,
}
impl State {
    pub fn new(dbc: Arc<Dbc>, cp: car_params::Reader<'_>, now: u64) -> Result<Self, Error> {
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            pt: Parser::new(Arc::clone(&dbc), 0, now),
            camera: Parser::new(Arc::clone(&dbc), 2, now),
            alt: Parser::new(Arc::clone(&dbc), 1, now),
            out,
            extras: Extras::default(),
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            flags: cp.get_flags(),
            longitudinal: cp.get_openpilot_longitudinal_control(),
            pcm: cp.get_pcm_cruise(),
            bsm: cp.get_enable_bsm(),
            factor: f64::from(cp.get_wheel_speed_factor()),
            defs: dbc.definitions()?,
            speed: SpeedFilter::new()?,
            cluster_seen: false,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            speed_filter: self.speed.state(),
            cluster_seen: self.cluster_seen,
        }
    }
    pub(super) fn parser(&mut self, bus: Bus) -> &mut Parser {
        match bus {
            Bus::Pt => &mut self.pt,
            Bus::Cam => &mut self.camera,
            Bus::Alt => &mut self.alt,
        }
    }
    pub(super) fn signal(
        &mut self,
        bus: Bus,
        name: &str,
        key: &str,
        now: u64,
    ) -> Result<f64, Error> {
        Ok(self.parser(bus).signal_lazy(name, key, now)?)
    }
    pub(super) fn copied(&mut self, bus: Bus, name: &str, now: u64) -> Result<Values, Error> {
        let parser = self.parser(bus);
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
    pub(super) fn chassis_bus(&self) -> Bus {
        if self.flags & GLOBAL_GEN2 != 0 {
            Bus::Alt
        } else {
            Bus::Pt
        }
    }
    pub(super) fn distance_bus(&self) -> Bus {
        if self.flags & (GLOBAL_GEN2 | HYBRID) != 0 {
            Bus::Alt
        } else {
            Bus::Cam
        }
    }
    fn drain_logs(&mut self) {
        for parser in [&mut self.pt, &mut self.camera, &mut self.alt] {
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
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        self.alt.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        self.motion(&mut ret, now)?;
        self.cruise(&mut ret, now)?;
        ret.set_can_valid(self.pt.can_valid() && self.camera.can_valid() && self.alt.can_valid());
        ret.set_can_timeout(
            self.pt.bus_timeout() || self.camera.bus_timeout() || self.alt.bus_timeout(),
        );
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        let speed = ret.reborrow_as_reader().get_cruise_state()?.get_speed();
        ret.reborrow().get_cruise_state()?.set_speed_cluster(speed);
        ret.set_button_enable(crate::state_helpers::button_enable(
            self.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
