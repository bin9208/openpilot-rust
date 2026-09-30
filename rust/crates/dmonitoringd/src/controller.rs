use crate::{wire, Error};
use openpilot_messaging::state::State;
use openpilot_monitoring::DriverMonitoring;
use openpilot_params::Params;

pub const TOPICS: &[&str] = &[
    "driverStateV2",
    "liveCalibration",
    "carState",
    "selfdriveState",
    "modelV2",
];

pub struct Controller {
    monitoring: DriverMonitoring,
    demo: bool,
}

pub struct Publication {
    pub bytes: Vec<u8>,
    pub frame_id: u32,
}

impl Controller {
    pub fn new(params: &Params) -> Result<Self, Error> {
        Ok(Self {
            monitoring: DriverMonitoring::new(
                params.get_bool("IsRhdDetected")?,
                params.get_bool("AlwaysOnDM")?,
                params.get_bool("DriverTooDistracted")?,
            ),
            demo: false,
        })
    }

    pub fn prepare(&mut self, state: &State) -> Result<Option<Publication>, Error> {
        let topic = state.topic("driverStateV2")?;
        if !topic.updated {
            return Ok(None);
        }
        let driver = wire::driver(state)?;
        let valid = state.all_checks(&[])?;
        if (self.demo && topic.valid) || valid {
            self.monitoring
                .run_step(&wire::input(state, driver, self.demo)?)?;
        }
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let timestamp = u64::try_from(now.tv_sec)
            .ok()
            .and_then(|value| value.checked_mul(1_000_000_000))
            .and_then(|value| value.checked_add(u64::try_from(now.tv_nsec).ok()?))
            .ok_or(Error::Contract("monotonic timestamp overflow"))?;
        Ok(Some(Publication {
            bytes: self.monitoring.state_packet(valid, timestamp)?,
            frame_id: driver.get_frame_id(),
        }))
    }

    pub fn after_publish(&mut self, frame_id: u32, params: &Params) -> Result<(), Error> {
        if frame_id % 40 == 1 {
            self.monitoring.always_on = params.get_bool("AlwaysOnDM")?;
            self.demo = params.get_bool("IsDriverViewEnabled")?;
        }
        if frame_id.is_multiple_of(6000)
            && !self.demo
            && self.monitoring.wheelpos_offsetter.filtered_stat.n > 300
            && self.monitoring.wheel_on_right
                == (self.monitoring.wheelpos_offsetter.filtered_stat.mean > 0.5)
        {
            params.put_bool("IsRhdDetected", self.monitoring.wheel_on_right)?;
        }
        Ok(())
    }
}
