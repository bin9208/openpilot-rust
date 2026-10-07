use super::{controller::Controller, state::State, Error};
use crate::{
    core::{ApplyInput, ApplyOutput, Message, VehicleLog},
    firmware_query::StartupIo,
    vehicle_params,
};
use openpilot_can::{dbc::Dbc, packer::Packer, Packet};
use openpilot_cereal::car_capnp::{car_params, car_state};
use openpilot_params::Params;
use std::{path::Path, sync::Arc};

pub struct Setup<'a> {
    pub params_bytes: &'a [u8],
    pub dbc_root: &'a Path,
    pub settings: Params,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub now_ns: u64,
}
pub struct Gm {
    pub state: State,
    pub controller: Controller,
}
impl Gm {
    pub fn new(setup: Setup<'_>) -> Result<Self, Error> {
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(setup.params_bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let cp = reader.get_root::<car_params::Reader>()?;
        let candidate = cp.get_car_fingerprint()?.to_str()?;
        let platform = vehicle_params::platform(candidate)?;
        if platform.brand != "gm" {
            return Err(Error::Platform(candidate.into()));
        }
        let name = platform
            .dbc_pt
            .ok_or_else(|| Error::Platform(candidate.into()))?;
        let dbc = Arc::new(Dbc::load(&setup.dbc_root.join(format!("{name}.dbc")))?);
        let settings = Arc::new(setup.settings);
        let state = State::new(Arc::clone(&dbc), cp, Arc::clone(&settings), setup.now_ns)?;
        let object = Arc::new(Dbc::load(&setup.dbc_root.join("gm_global_a_object.dbc"))?);
        let chassis = Arc::new(Dbc::load(&setup.dbc_root.join("gm_global_a_chassis.dbc"))?);
        let controller = Controller::new(
            Packer::new(dbc),
            Packer::new(object),
            Packer::new(chassis),
            cp,
            settings,
        )?;
        Ok(Self { state, controller })
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.state.update(packets, now)
    }
    pub fn apply(&mut self, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        self.controller.apply(&self.state, input)
    }
    pub fn commit_state(&mut self, state: car_state::Reader<'_>) -> Result<(), Error> {
        self.state.out.set_root(state)?;
        Ok(())
    }
    pub fn set_soft_hold(&mut self, active: i16) {
        self.state.soft_hold = active;
    }
    pub fn init(&mut self, _io: &mut impl StartupIo) -> Result<(), Error> {
        Ok(())
    }
    pub fn deinit(&mut self, _io: &mut impl StartupIo) -> Result<(), Error> {
        Ok(())
    }
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        Vec::new()
    }
    pub fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
    pub fn take_logs(&mut self) -> Vec<VehicleLog> {
        let mut logs = std::mem::take(&mut self.state.logs);
        logs.append(&mut self.controller.logs);
        logs
    }
    pub fn take_param_writes(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.controller.writes)
    }
}
