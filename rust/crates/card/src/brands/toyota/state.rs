use super::{config::Config, state_motion::float, Error, RADAR_ACC, TSS2};
use crate::{
    core::{Message, VehicleLog},
    query::DiagnosticLevel,
    state_helpers::{self, SpeedFilter},
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
#[derive(Serialize)]
pub struct Extras {
    pub accurate_steer_angle_seen: bool,
    pub angle_offset: Option<f64>,
    pub distance_button: f64,
    pub pcm_follow_distance: f64,
    pub pcm_acc_status: Option<f64>,
    pub acc_type: f64,
    pub lkas_hud: Values,
    pub gvc: f64,
    pub secoc_synchronization: Option<Values>,
}
impl Default for Extras {
    fn default() -> Self {
        Self {
            accurate_steer_angle_seen: false,
            angle_offset: None,
            distance_button: 0.,
            pcm_follow_distance: 0.,
            pcm_acc_status: None,
            acc_type: 1.,
            lkas_hud: Values::new(),
            gvc: 0.,
            secoc_synchronization: None,
        }
    }
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
    pub(super) defs: Definitions,
    pub(super) speed: SpeedFilter,
    cluster_seen: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    speed_filter: [f64; 2],
    cluster_seen: bool,
}
impl State {
    pub fn new(dbc: Arc<Dbc>, cp: car_params::Reader<'_>, now: u64) -> Result<Self, Error> {
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        let mut pt = Parser::new(Arc::clone(&dbc), 0, now);
        pt.add("BLINKERS_STATE", Some(f64::NAN), false, now)?;
        Ok(Self {
            pt,
            camera: Parser::new(Arc::clone(&dbc), 2, now),
            out,
            extras: Extras::default(),
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            config: Config::new(cp)?,
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
    pub(super) fn signal(
        &mut self,
        camera: bool,
        name: &str,
        key: &str,
        now: u64,
    ) -> Result<f64, Error> {
        let parser = if camera {
            &mut self.camera
        } else {
            &mut self.pt
        };
        Ok(parser.signal_lazy(name, key, now)?)
    }
    pub(super) fn copied(&mut self, camera: bool, name: &str, now: u64) -> Result<Values, Error> {
        let parser = if camera {
            &mut self.camera
        } else {
            &mut self.pt
        };
        let keys = parser
            .dbc
            .message(name)?
            .signals
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>();
        keys.into_iter()
            .map(|key| Ok((key.clone(), parser.signal_lazy(name, &key, now)?)))
            .collect()
    }
    pub(super) fn acc_camera(&self) -> bool {
        self.config.static_flags & TSS2 != 0 && self.config.static_flags & RADAR_ACC == 0
    }
    fn drain_logs(&mut self) {
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
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        let gear = self.body(&mut ret, now)?;
        self.motion(&mut ret, gear, now)?;
        self.cruise(&mut ret, now)?;
        ret.set_can_valid(self.pt.can_valid() && self.camera.can_valid());
        ret.set_can_timeout(self.pt.bus_timeout() || self.camera.bus_timeout());
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        let current = f64::from(ret.reborrow_as_reader().get_v_ego_cluster());
        let previous = f64::from(
            self.out
                .get_root_as_reader::<car_state::Reader>()?
                .get_v_ego_cluster(),
        );
        let gap = (1. / 3.6) / 2.;
        let cluster = if current > previous + gap {
            current - gap
        } else if current < previous - gap {
            current + gap
        } else {
            previous
        };
        ret.set_v_ego_cluster(
            if f64::from(ret.reborrow_as_reader().get_v_ego().abs()) < gap {
                0.
            } else {
                float(cluster)?
            },
        );
        if ret
            .reborrow_as_reader()
            .get_cruise_state()?
            .get_speed_cluster()
            == 0.
        {
            let speed = ret.reborrow_as_reader().get_cruise_state()?.get_speed();
            ret.reborrow().get_cruise_state()?.set_speed_cluster(speed);
        }
        ret.set_button_enable(state_helpers::button_enable(
            self.config.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
