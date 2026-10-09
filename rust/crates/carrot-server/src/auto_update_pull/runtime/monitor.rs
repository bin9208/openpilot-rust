use crate::{
    auto_update::{ManagerReady, RebootSample, READY_DELAY},
    git_state::Time,
    Error, Value,
};
use num_traits::ToPrimitive;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use tokio::sync::watch;

type Messaging = dyn Fn(&[&str]) -> Result<SubMaster, Error> + Send + Sync;

#[derive(Clone)]
pub struct Inputs {
    pub messaging: Arc<Messaging>,
    pub monotonic: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub wall: Arc<dyn Fn() -> Time + Send + Sync>,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            messaging: Arc::new(|names| {
                SubMaster::for_runtime(names, Options::default())
                    .map_err(|error| Error::Source(error.to_string()))
            }),
            monotonic: Arc::new(|| {
                let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
                now.tv_sec.to_f64().unwrap_or(0.) + now.tv_nsec.to_f64().unwrap_or(0.) / 1e9
            }),
            wall: Arc::new(|| {
                let now = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
                Time {
                    seconds: Value::integer(now.tv_sec),
                    nanoseconds: Value::integer(
                        num_bigint::BigInt::from(now.tv_sec) * 1_000_000_000 + now.tv_nsec,
                    ),
                }
            }),
        }
    }
}

struct State {
    sm: Option<SubMaster>,
    condition: ManagerReady,
}

pub struct ManagerMonitor {
    inputs: Inputs,
    state: RefCell<State>,
}

impl ManagerMonitor {
    pub fn new(inputs: Inputs) -> Rc<Self> {
        Rc::new(Self {
            inputs,
            state: RefCell::new(State {
                sm: None,
                condition: ManagerReady::new(READY_DELAY),
            }),
        })
    }

    pub fn ready(&self) -> bool {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return false;
        };
        let sample = || -> Result<bool, Error> {
            if state.sm.is_none() {
                state.sm = Some((self.inputs.messaging)(&["managerState"])?);
            }
            let sm = state
                .sm
                .as_mut()
                .ok_or_else(|| Error::Source("manager subscription unavailable".into()))?;
            sm.update(Duration::ZERO)
                .map_err(|error| Error::Source(error.to_string()))?;
            Ok(valid(sm, "managerState"))
        }();
        let valid = match sample {
            Ok(valid) => valid,
            Err(_) => {
                state.sm = None;
                false
            }
        };
        state.condition.update((self.inputs.monotonic)(), valid)
    }

    pub(super) async fn observe(self: Rc<Self>, mut stopped: watch::Receiver<bool>) {
        while !*stopped.borrow() {
            self.ready();
            if !super::sleep(Duration::from_secs(1), &mut stopped).await {
                break;
            }
        }
    }
}

fn valid(sm: &SubMaster, name: &str) -> bool {
    sm.state
        .topic(name)
        .is_ok_and(|topic| topic.valid && topic.alive)
}

pub(super) struct Signals {
    selfdrive: bool,
    engaged: bool,
    car: bool,
    gear: Value,
    device: bool,
    started: bool,
}

impl Signals {
    pub(super) fn sample(&self, now: f64) -> RebootSample<'_> {
        RebootSample {
            now,
            selfdrive_valid: self.selfdrive,
            engaged: self.engaged,
            car_state_valid: self.car,
            gear_shifter: &self.gear,
            device_state_valid: self.device,
            device_started: self.started,
        }
    }
}

pub(super) fn signals(sm: &SubMaster) -> Result<Signals, Error> {
    let selfdrive = valid(sm, "selfdriveState");
    let car = valid(sm, "carState");
    let device = valid(sm, "deviceState");
    let fail = |error: openpilot_messaging::state::Error| Error::Source(error.to_string());
    let engaged = if selfdrive {
        let openpilot_cereal::log_capnp::event::SelfdriveState(value) = sm
            .state
            .topic("selfdriveState")
            .map_err(fail)?
            .event()
            .map_err(fail)?
            .which()
            .map_err(|error| Error::Source(error.to_string()))?
        else {
            return Err(Error::Source("selfdriveState event required".into()));
        };
        value
            .map_err(|error| Error::Source(error.to_string()))?
            .get_enabled()
    } else {
        false
    };
    let gear = if car {
        let event = sm
            .state
            .topic("carState")
            .map_err(fail)?
            .event()
            .map_err(fail)?;
        let openpilot_cereal::log_capnp::event::CarState(value) = event
            .which()
            .map_err(|error| Error::Source(error.to_string()))?
        else {
            return Err(Error::Source("carState event required".into()));
        };
        let gear = value
            .map_err(|error| Error::Source(error.to_string()))?
            .get_gear_shifter();
        Value::text(
            if matches!(
                gear,
                Ok(openpilot_cereal::car_capnp::car_state::GearShifter::Park)
            ) {
                "park"
            } else {
                "other"
            },
        )
    } else {
        Value::Null
    };
    let started = if device {
        let openpilot_cereal::log_capnp::event::DeviceState(value) = sm
            .state
            .topic("deviceState")
            .map_err(fail)?
            .event()
            .map_err(fail)?
            .which()
            .map_err(|error| Error::Source(error.to_string()))?
        else {
            return Err(Error::Source("deviceState event required".into()));
        };
        value
            .map_err(|error| Error::Source(error.to_string()))?
            .get_started()
    } else {
        true
    };
    Ok(Signals {
        selfdrive,
        engaged,
        car,
        gear,
        device,
        started,
    })
}
