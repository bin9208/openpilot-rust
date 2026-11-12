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
    /// Automatic turn-signal lamp session (carrot `TeslaTurnSignals`).
    pub turn_signals: super::turn_signals::TurnSignals,
    urgent: Vec<openpilot_can::Frame>,
}

/// carrot `TeslaTurnSignals.from_car_params`.
fn turn_signals(cp: car_params::Reader<'_>, enabled: bool) -> super::turn_signals::TurnSignals {
    let model = cp
        .get_car_fingerprint()
        .ok()
        .and_then(|t| t.to_str().ok())
        .unwrap_or_default();
    let safety = cp.get_safety_configs().is_ok_and(|configs| {
        configs.iter().any(|c| {
            c.get_safety_model() == Ok(car_params::SafetyModel::Tesla)
                && c.get_safety_param() & 8 != 0
        })
    });
    let reason = if cp.get_brand().ok().and_then(|t| t.to_str().ok()) != Some("tesla") {
        Some("disabled:not_tesla")
    } else if !enabled {
        Some("disabled:setting_off")
    } else if !matches!(model, "TESLA_MODEL_3" | "TESLA_MODEL_Y") {
        Some("disabled:unsupported_platform")
    } else if cp.get_passive() {
        Some("disabled:passive")
    } else if cp.get_flags() & super::HAS_VEHICLE_BUS == 0 {
        Some("blocked:vehicle_bus_unavailable")
    } else if cp.get_flags() & super::AUTO_TURN_SIGNALS == 0 {
        Some("blocked:restart_required")
    } else if !safety {
        Some("blocked:safety_configuration")
    } else {
        None
    };
    super::turn_signals::TurnSignals::new(reason.is_none(), reason.unwrap_or("disabled"))
}

fn signal_car(cs: car_state::Reader<'_>) -> super::turn_signals::Car {
    super::turn_signals::Car {
        driver_valid: cs.get_turn_signal_driver_valid(),
        driver_direction: cs.get_turn_signal_driver_direction(),
        stalk_counts: [
            cs.get_left_blinker_stalk_count(),
            cs.get_right_blinker_stalk_count(),
        ],
        cruise_cancel: cs.get_cruise_cancel_state(),
        can_valid: cs.get_can_valid(),
        sensors_invalid: cs.get_vehicle_sensors_invalid(),
        drive: cs.get_gear_shifter() == Ok(car_state::GearShifter::Drive),
        v_ego: f64::from(cs.get_v_ego()),
        trailer: cs.get_trailer_connected(),
        steer_fault: cs.get_steer_fault_temporary() || cs.get_steer_fault_permanent(),
        standstill: cs.get_standstill(),
    }
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
        let turn_signals = turn_signals(
            cp,
            setup
                .settings
                .get_bool("TeslaAutoTurnSignals")
                .unwrap_or(false),
        );
        Ok(Self {
            state,
            controller,
            turn_signals,
            urgent: Vec::new(),
        })
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        let mut message = self.state.update(packets, now)?;
        let car = signal_car(message.get_root_as_reader::<car_state::Reader<'_>>()?);
        let signals = &mut self.turn_signals;
        signals.observe_driver(&car, now);
        signals.observe(packets, now);
        // carrot sends a pending cancel at once; it leads the next output.
        self.urgent.extend(signals.take_sends(now, true));
        let mut cs = message.get_root::<car_state::Builder<'_>>()?;
        cs.set_auto_turn_signal_direction(signals.owned_direction());
        cs.set_auto_turn_signal_ready_request_id(signals.ready_request_id(now));
        cs.set_auto_turn_signal_status(signals.status.as_str());
        Ok(message)
    }
    pub fn apply(&mut self, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let now = input.now_ns;
        let request = if self.turn_signals.configured {
            let seconds = now as f64 * 1e-9;
            let fresh = input.subscribers.is_some_and(|subs| {
                ["carControl", "modelV2"].iter().all(|name| {
                    subs.topic(name).is_ok_and(|t| {
                        t.seen
                            && t.alive
                            && t.valid
                            && (0. ..=0.2).contains(&(seconds - t.receive_time))
                    })
                })
            });
            let meta = input.model.and_then(|m| m.get_meta().ok());
            let direction = meta
                .as_ref()
                .and_then(|m| m.get_auto_turn_signal_direction().ok())
                .map_or(0, |d| u16::from(d) as u8);
            let turn_desire = meta
                .as_ref()
                .and_then(|m| m.get_desire().ok())
                .is_some_and(|d| {
                    use openpilot_cereal::log_capnp::Desire;
                    (direction == 1 && d == Desire::TurnLeft)
                        || (direction == 2 && d == Desire::TurnRight)
                });
            Some(super::turn_signals::Request {
                lat_active: input.control.get_lat_active(),
                direction,
                request_id: meta
                    .as_ref()
                    .map_or(0, |m| m.get_auto_turn_signal_request_id()),
                turn_desire,
                fresh,
                turn_hold_allowed: false,
                lamp_intent: None,
            })
        } else {
            None
        };
        let mut output = self.controller.apply(&mut self.state, input)?;
        if let Some(request) = request {
            let car = signal_car(
                self.state
                    .out
                    .get_root_as_reader::<car_state::Reader<'_>>()?,
            );
            output
                .can
                .extend(self.turn_signals.update(&car, &request, now));
        }
        if !self.urgent.is_empty() {
            let mut can = std::mem::take(&mut self.urgent);
            can.append(&mut output.can);
            output.can = can;
        }
        Ok(output)
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
