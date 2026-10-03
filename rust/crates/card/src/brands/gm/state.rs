use super::{float, model::Model, Error};
use crate::{
    brands::hyundai::parameters::setting_int,
    core::{Message, VehicleLog},
    query::DiagnosticLevel,
    state_helpers::SpeedFilter,
};
use openpilot_can::{
    dbc::{Dbc, Definitions},
    parser::Parser,
};
use openpilot_cereal::car_capnp::{
    car_params::{self, NetworkLocation, TransmissionType},
    car_state,
};
use openpilot_params::Params;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default, Serialize)]
pub struct Extras {
    pub loopback_lka_steering_cmd_updated: bool,
    pub loopback_lka_steering_cmd_ts_nanos: u64,
    pub pt_lka_steering_cmd_counter: f64,
    pub cam_lka_steering_cmd_counter: f64,
    pub buttons_counter: f64,
    pub single_pedal_mode: bool,
    pub pedal_steady: f64,
    pub cruise_buttons: f64,
    pub distance_button: f64,
    #[serde(rename = "cruiseMain_on")]
    pub cruise_main_on: bool,
    pub lkas_enabled: f64,
    pub pscm_status: Option<BTreeMap<String, f64>>,
    pub lkas_status: Option<f64>,
    pub pcm_acc_status: Option<f64>,
}
pub struct Config {
    pub model: Model,
    pub flags: u32,
    pub camera: bool,
    pub direct: bool,
    pub interceptor: bool,
    pub blindspots: bool,
    pub pcm: bool,
    pub factor: f64,
}
pub struct State {
    pub pt: Parser,
    pub camera: Parser,
    pub loopback: Parser,
    pub out: Message,
    pub extras: Extras,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
    pub is_metric: bool,
    pub(super) config: Config,
    pub(super) settings: Arc<Params>,
    pub(super) speed: SpeedFilter,
    pub(super) cluster_speed: SpeedFilter,
    pub(super) defs: Definitions,
    pub(super) cluster_seen: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    speed_filter: [f64; 2],
    cluster_filter: [f64; 2],
    cluster_seen: bool,
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
            cluster_filter: self.cluster_speed.state(),
            cluster_seen: self.cluster_seen,
        }
    }
    pub fn new(
        dbc: Arc<Dbc>,
        cp: car_params::Reader<'_>,
        settings: Arc<Params>,
        now: u64,
    ) -> Result<Self, Error> {
        let defs = dbc.definitions()?;
        let config = Config {
            model: Model::new(cp.get_car_fingerprint()?.to_str()?)?,
            flags: cp.get_flags(),
            camera: cp.get_network_location()? == NetworkLocation::FwdCamera,
            direct: cp.get_transmission_type()? == TransmissionType::Direct,
            interceptor: cp.get_enable_gas_interceptor_d_e_p_r_e_c_a_t_e_d(),
            blindspots: cp.get_enable_bsm(),
            pcm: cp.get_pcm_cruise(),
            factor: f64::from(cp.get_wheel_speed_factor()),
        };
        let mut pt = Parser::new(Arc::clone(&dbc), 0, now);
        if config.camera {
            pt.add("ASCMLKASteeringCmd", Some(f64::NAN), false, now)?;
        }
        if config.direct {
            pt.add("EBCMRegenPaddle", Some(50.), false, now)?;
            pt.add("EVDriveMode", Some(f64::NAN), false, now)?;
        }
        let camera = Parser::new(Arc::clone(&dbc), 2, now);
        let mut loopback = Parser::new(dbc, 128, now);
        loopback.add("ASCMLKASteeringCmd", Some(f64::NAN), false, now)?;
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        let extras = Extras {
            cruise_main_on: setting_int(&settings, "AutoEngage")? == 2,
            ..Extras::default()
        };
        Ok(Self {
            pt,
            camera,
            loopback,
            out,
            extras,
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: false,
            config,
            settings,
            speed: SpeedFilter::new()?,
            cluster_speed: SpeedFilter::new()?,
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
        for parser in [&mut self.pt, &mut self.camera, &mut self.loopback] {
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
    pub(super) fn finish(&mut self, mut ret: car_state::Builder<'_>) -> Result<(), Error> {
        ret.set_can_valid(
            self.pt.can_valid() && self.camera.can_valid() && self.loopback.can_valid(),
        );
        ret.set_can_timeout(
            self.pt.bus_timeout() || self.camera.bus_timeout() || self.loopback.bus_timeout(),
        );
        let mut cluster = f64::from(ret.reborrow_as_reader().get_v_ego_cluster());
        if cluster == 0. && !self.cluster_seen {
            cluster = f64::from(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        let previous = f64::from(
            self.out
                .get_root_as_reader::<car_state::Reader>()?
                .get_v_ego_cluster(),
        );
        let gap = (1. / 3.6) / 2.;
        cluster = if cluster > previous + gap {
            cluster - gap
        } else if cluster < previous - gap {
            cluster + gap
        } else {
            previous
        };
        ret.set_v_ego_cluster(
            if f64::from(ret.reborrow_as_reader().get_v_ego()).abs() < gap {
                0.
            } else {
                float(cluster)?
            },
        );
        let mut cruise = ret.reborrow().get_cruise_state()?;
        if cruise.reborrow_as_reader().get_speed_cluster() == 0. {
            cruise.set_speed_cluster(cruise.reborrow_as_reader().get_speed());
        }
        let mut enable = false;
        if !self.config.pcm {
            for event in ret.reborrow_as_reader().get_button_events()? {
                use car_state::button_event::Type;
                enable |= (event.get_type()? == Type::AccelCruise && event.get_pressed())
                    || (event.get_type()? == Type::DecelCruise && !event.get_pressed());
            }
        }
        ret.set_button_enable(enable);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(())
    }
}
