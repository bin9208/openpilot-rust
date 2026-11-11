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
    pub spas: super::spas_signals::SpasSignals,
    spas_logged: (String, f64),
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
        let spas = super::spas_signals::SpasSignals::new(
            state.config.flags & f::CANFD_SPAS_BLINKERS != 0,
            state.config.bus.ecan,
            &state.config.candidate,
        );
        Ok(Self {
            state,
            controller,
            spas,
            spas_logged: (String::new(), 0.),
        })
    }

    pub fn update(
        &mut self,
        packets: &[Packet],
        now_ns: u64,
    ) -> Result<Builder<HeapAllocator>, Error> {
        let message = self.state.update(packets, now_ns)?;
        self.spas.observe(packets, now_ns as f64 * 1e-9);
        Ok(message)
    }
    pub fn apply(&mut self, input: super::ApplyInput<'_>) -> Result<super::ApplyOutput, Error> {
        let mut spas_sends = Vec::new();
        if self.spas.enabled {
            if let Some(subscribers) = input.subscribers {
                let now = input.now_ns as f64 * 1e-9;
                let cs = self
                    .state
                    .out
                    .get_root_as_reader::<car_state::Reader<'_>>()?;
                spas_sends = self.spas.update(super::spas_signals::Inputs {
                    cs,
                    subscribers,
                    settings: &self.state.settings,
                    now,
                    initialized: input.initialized,
                    controller_frame: u64::from(self.controller.frame),
                    writer: &mut self.controller.writer,
                    bus: self.state.config.bus,
                })?;
                // carrot card.py: CC.hudControl.physicalBlinkerHold = hold_value.
                self.controller.external_hold = self.spas.hold_value;
                self.log_spas(cs_blinkers(&self.state)?, now);
            }
        }
        let mut output = self.controller.apply(&mut self.state, input)?;
        if !spas_sends.is_empty() {
            spas_sends.append(&mut output.can);
            output.can = spas_sends;
        }
        Ok(output)
    }
    /// carrot card.py `cloudlog.event('hyundai_spas', ...)`: on every status
    /// change, and once a second while a session or a parked trial runs.
    fn log_spas(&mut self, blinkers: (bool, bool), now: f64) {
        let s = &self.spas;
        let running = s.session.state != super::spas::State::Idle
            || (s.test_direct && s.test_request.is_some());
        if s.status != self.spas_logged.0 || (running && now >= self.spas_logged.1) {
            self.state.inputs.diagnostics.logs.push(crate::core::VehicleLog {
                level: crate::query::DiagnosticLevel::Info,
                message: format!(
                    "hyundai_spas state={} reason={} request={:?} direction={:?} left={} right={} hold={} trial={} trial_status={} restored_ack={} stock_resumed={}",
                    s.session.state.name(),
                    s.session.reason,
                    s.session.request(),
                    s.session.direction(),
                    blinkers.0,
                    blinkers.1,
                    s.hold_value,
                    s.test_setting,
                    s.status,
                    s.session.restore_ack,
                    s.session.stock_resumed,
                ),
            });
            self.spas_logged = (s.status.clone(), now + 1.);
        }
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
    pub fn take_logs(&mut self) -> Vec<crate::core::VehicleLog> {
        let mut logs = std::mem::take(&mut self.state.inputs.diagnostics.logs);
        logs.append(&mut self.controller.steering.logs);
        logs
    }
    pub fn take_warnings(&mut self) -> Vec<String> {
        let mut warnings = std::mem::take(&mut self.state.inputs.diagnostics.warnings);
        if let Some(stopping) = &mut self.controller.stopping {
            warnings.append(&mut stopping.warnings);
        }
        warnings
    }
}

fn cs_blinkers(state: &State) -> Result<(bool, bool), Error> {
    let cs = state.out.get_root_as_reader::<car_state::Reader<'_>>()?;
    Ok((cs.get_left_blinker(), cs.get_right_blinker()))
}
