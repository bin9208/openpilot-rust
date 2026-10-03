use super::{
    controller::Controller,
    state::{Config, State},
    Error,
};
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
pub struct Ford {
    pub state: State,
    pub controller: Controller,
}
impl Ford {
    pub fn new(setup: Setup<'_>) -> Result<Self, Error> {
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(setup.params_bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let cp = reader.get_root::<car_params::Reader>()?;
        let candidate = cp.get_car_fingerprint()?.to_str()?;
        if vehicle_params::platform(candidate)?.brand != "ford" {
            return Err(Error::Platform(candidate.into()));
        }
        let main = u8::try_from(
            cp.get_safety_configs()?
                .len()
                .checked_sub(1)
                .ok_or(Error::Numeric)?
                * 4,
        )
        .map_err(|_| Error::Numeric)?;
        let dbc = Arc::new(Dbc::load(&setup.dbc_root.join("ford_lincoln_base_pt.dbc"))?);
        let config = Config {
            main,
            canfd: cp.get_flags() & 1 != 0,
            longitudinal: cp.get_openpilot_longitudinal_control(),
            pcm: cp.get_pcm_cruise(),
            blindspots: cp.get_enable_bsm(),
            transmission: cp.get_transmission_type()?,
        };
        let controller = Controller::new(
            Packer::new(Arc::clone(&dbc)),
            main,
            config.canfd,
            config.longitudinal,
        );
        Ok(Self {
            state: State::new(dbc, config, setup.now_ns)?,
            controller,
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
}
