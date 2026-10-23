use super::{controller::Controller, state::State, Error};
use crate::{
    core::{ApplyInput, ApplyOutput, Message, VehicleLog},
    firmware_query::StartupIo,
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
pub struct Psa {
    pub state: State,
    pub controller: Controller,
}
impl Psa {
    pub fn new(input: Setup<'_>) -> Result<Self, Error> {
        let message = capnp::serialize::read_message(
            std::io::Cursor::new(input.params_bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let cp = message.get_root::<car_params::Reader>()?;
        let candidate = cp.get_car_fingerprint()?.to_str()?;
        if candidate != "PSA_PEUGEOT_208" {
            return Err(Error::Platform(candidate.to_owned()));
        }
        let path = input.dbc_root.join("psa_aee2010_r3.dbc");
        let dbc = Arc::new(Dbc::load(&path).map_err(|source| Error::DbcLoad { path, source })?);
        Ok(Self {
            state: State::new(
                Arc::clone(&dbc),
                f64::from(cp.get_wheel_speed_factor()),
                input.now_ns,
            )?,
            controller: Controller::new(Packer::new(dbc)),
        })
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
    pub fn init(&mut self, _: &mut impl StartupIo) -> Result<(), Error> {
        Ok(())
    }
    pub fn deinit(&mut self, _: &mut impl StartupIo) -> Result<(), Error> {
        Ok(())
    }
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        Vec::new()
    }
    pub fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
    pub fn take_logs(&mut self) -> Vec<VehicleLog> {
        std::mem::take(&mut self.state.logs)
    }
}
