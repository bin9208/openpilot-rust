use super::{controller::Controller, state::State, Error};
use crate::{
    core::{ApplyInput, ApplyOutput, Message, VehicleLog},
    firmware_query::StartupIo,
};
use openpilot_can::Packet;
use openpilot_cereal::car_capnp::{car_params, car_state};
use openpilot_params::Params;
use std::path::Path;

pub struct Setup<'a> {
    pub params_bytes: &'a [u8],
    pub dbc_root: &'a Path,
    pub settings: Params,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub now_ns: u64,
}
pub struct Tesla {
    pub state: State,
    pub controller: Controller,
}
impl Tesla {
    pub fn new(setup: Setup<'_>) -> Result<Self, Error> {
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(setup.params_bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let cp = reader.get_root::<car_params::Reader>()?;
        let state = State::new(
            setup.dbc_root,
            cp.get_flags(),
            f64::from(cp.get_steer_ratio()),
            cp.get_pcm_cruise(),
            setup.now_ns,
        )?;
        let controller = Controller::new(cp, &state, &setup.settings)?;
        Ok(Self { state, controller })
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.state.update(packets, now)
    }
    pub fn apply(&mut self, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        self.controller.apply(&mut self.state, input)
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
        std::mem::take(&mut self.state.prints)
    }
    pub fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
    pub fn take_logs(&mut self) -> Vec<VehicleLog> {
        std::mem::take(&mut self.state.logs)
    }
}
