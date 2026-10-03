use super::{
    bus::Fingerprints,
    config::CarConfig,
    controller::Controller,
    flags as f,
    state::{CanSetup, State},
    Error,
};
use capnp::message::{Builder, HeapAllocator};
use openpilot_can::Packet;
use openpilot_cereal::car_capnp::car_state;
use openpilot_params::Params;
use std::path::Path;

pub struct Setup<'a> {
    pub params_bytes: &'a [u8],
    pub dbc_root: &'a Path,
    pub settings: Params,
    pub fingerprints: &'a Fingerprints,
    pub now_ns: u64,
}

pub struct Hyundai {
    pub state: State,
    pub controller: Controller,
}

impl Hyundai {
    pub fn new(setup: Setup<'_>) -> Result<Self, Error> {
        let config = CarConfig::decode(setup.params_bytes, &setup.settings)?;
        let path = setup.dbc_root.join(if config.flags & f::CANFD != 0 {
            "hyundai_canfd_generated.dbc"
        } else {
            "hyundai_kia_generic.dbc"
        });
        let state = State::new(
            config,
            setup.settings,
            CanSetup {
                path: &path,
                fingerprints: setup.fingerprints,
                now_ns: setup.now_ns,
            },
        )?;
        let controller = Controller::new(&state)?;
        Ok(Self { state, controller })
    }

    pub fn update(
        &mut self,
        packets: &[Packet],
        now_ns: u64,
    ) -> Result<Builder<HeapAllocator>, Error> {
        self.state.update(packets, now_ns)
    }
    pub fn apply(&mut self, input: super::ApplyInput<'_>) -> Result<super::ApplyOutput, Error> {
        self.controller.apply(&mut self.state, input)
    }
    pub fn init(&mut self, io: &mut impl crate::firmware_query::StartupIo) -> Result<(), Error> {
        super::startup::init(&mut self.state, io)
    }
    pub fn deinit(&mut self, _io: &mut impl crate::firmware_query::StartupIo) -> Result<(), Error> {
        Ok(())
    }
    pub fn set_soft_hold(&mut self, active: i16) {
        self.state.soft_hold = active;
    }
    pub fn commit_state(&mut self, state: car_state::Reader<'_>) -> Result<(), Error> {
        self.state.commit(state)
    }
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        let mut lines = std::mem::take(&mut self.state.inputs.diagnostics.prints);
        lines.append(&mut self.controller.buttons.diagnostics);
        lines
    }
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.inputs.diagnostics.warnings)
    }
}
