pub(super) use super::state_access::{float, required, Bus};
pub use super::state_data::{Extras, Stock, Values};
use super::{
    config::{Config, Family},
    Error,
};
use crate::{
    core::{Message, VehicleLog},
    state_helpers::SpeedFilter,
};
use openpilot_can::{dbc::Dbc, parser::Parser};
use openpilot_cereal::car_capnp::{car_params::TransmissionType, car_state};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};
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
    pub(super) cluster_seen: bool,
    pub(super) gears: Option<BTreeMap<i64, String>>,
    pub(super) hca: BTreeMap<i64, String>,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub(super) speed_filter: [f64; 2],
    pub(super) cluster_seen: bool,
}
impl State {
    pub fn new(dbc: Arc<Dbc>, config: Config, now: u64) -> Result<Self, Error> {
        let definitions = dbc.definitions()?;
        let definition = |name, key| -> Result<BTreeMap<i64, String>, Error> {
            let address = dbc.message(name)?.address;
            definitions
                .get(&address)
                .and_then(|m| m.get(key))
                .cloned()
                .ok_or_else(|| Error::Signal(format!("{name}.{key} definitions")))
        };
        let gears = match config.family {
            Family::Meb => Some(definition("Getriebe_11", "GE_Fahrstufe")?),
            Family::Pq => {
                if config.transmission == TransmissionType::Automatic {
                    Some(definition("Getriebe_1", "Waehlhebelposition__Getriebe_1_")?)
                } else {
                    None
                }
            }
            Family::Mqb => match config.transmission {
                TransmissionType::Automatic => Some(definition("Gateway_73", "GE_Fahrstufe")?),
                TransmissionType::Direct => Some(definition("Motor_EV_01", "MO_Waehlpos")?),
                TransmissionType::Manual | TransmissionType::Unknown | TransmissionType::Cvt => {
                    None
                }
            },
        };
        let hca = match config.family {
            Family::Meb => definition("QFK_01", "LatCon_HCA_Status")?,
            Family::Pq => definition("Lenkhilfe_2", "LH2_Sta_HCA")?,
            Family::Mqb => definition("LH_EPS_03", "EPS_HCA_Status")?,
        };
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        let mut state = Self {
            pt: Parser::new(Arc::clone(&dbc), 0, now),
            camera: Parser::new(dbc, 2, now),
            out,
            extras: Extras::default(),
            logs: Vec::new(),
            soft_hold: 0,
            is_metric: true,
            config,
            speed: SpeedFilter::new()?,
            cluster_seen: false,
            gears,
            hca,
        };
        state.register(now)?;
        Ok(state)
    }
}
