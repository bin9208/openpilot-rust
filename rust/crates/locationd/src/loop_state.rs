use crate::{
    estimator::Estimator,
    types::{Event, HandleResult, Pose},
    wire, Error,
};
use openpilot_messaging::state::State;

const CRITICAL: [&str; 3] = ["accelerometer", "gyroscope", "cameraOdometry"];

pub struct LoopState {
    pub estimator: Estimator,
    pub initialized: bool,
    pub invalid: [f64; 3],
    pub sensor_valid: [bool; 2],
    pub sensor_alive: [bool; 2],
    pub sensor_received: [f64; 2],
    thresholds: [f64; 3],
    decay: [f64; 3],
    reasons: [&'static str; 3],
    last_print: f64,
    simulation: bool,
}
pub struct Output {
    pub pose: Pose,
    pub diagnostic: Option<String>,
}
impl LoopState {
    pub fn new(debug: bool, simulation: bool) -> Result<Self, Error> {
        let mut thresholds = [0.; 3];
        let mut decay = [0.; 3];
        for (i, name) in CRITICAL.into_iter().enumerate() {
            let frequency = openpilot_messaging::services::lookup(name)
                .ok_or(Error::Contract("critical service"))?
                .frequency;
            let limit = (2. * frequency / 20.).round_ties_even();
            thresholds[i] = limit - 0.5;
            decay[i] = (1. - 1. / (2. * limit)).powf(1. / (10. * frequency));
        }
        Ok(Self {
            estimator: Estimator::new(debug)?,
            initialized: false,
            invalid: [0.; 3],
            sensor_valid: [false; 2],
            sensor_alive: [false; 2],
            sensor_received: [0.; 2],
            thresholds,
            decay,
            reasons: ["unknown"; 3],
            last_print: 0.,
            simulation,
        })
    }
    fn sensor_checks(&mut self, sensors: [&[Event]; 2], now: f64) -> bool {
        for (i, events) in sensors.into_iter().enumerate() {
            if let Some(event) = events.last() {
                self.sensor_valid[i] = event.valid;
                self.sensor_received[i] = now;
            }
            self.sensor_alive[i] = if self.simulation {
                !events.is_empty()
            } else {
                now - self.sensor_received[i] < 0.1
            };
        }
        self.sensor_alive
            .into_iter()
            .chain(self.sensor_valid)
            .all(|v| v)
    }
    pub fn step(
        &mut self,
        state: &State,
        acceleration: &[Event],
        gyroscope: &[Event],
        mut clock: impl FnMut() -> f64,
    ) -> Result<Option<Output>, Error> {
        if self.initialized {
            let mut messages: Vec<_> = acceleration.iter().chain(gyroscope).collect();
            let mut updated = Vec::new();
            for topic in state.topics() {
                if topic.updated {
                    updated.push(wire::decode_event(topic.event()?)?);
                }
            }
            messages.extend(&updated);
            messages.sort_by_key(|event| event.log_time_ns);
            for event in messages {
                if !event.valid {
                    continue;
                }
                let result = self
                    .estimator
                    .handle(event.log_time_ns as f64 * 1e-9, event.input)?;
                let Some(index) = CRITICAL.iter().position(|name| *name == event.service) else {
                    continue;
                };
                let reason = match result {
                    HandleResult::TimingInvalid => Some("timing check"),
                    HandleResult::InputInvalid => Some("sanity check"),
                    HandleResult::Success => {
                        self.invalid[index] *= self.decay[index];
                        None
                    }
                    HandleResult::SensorSourceInvalid => None,
                };
                if let Some(reason) = reason {
                    self.invalid[index] += 1.;
                    self.reasons[index] = reason;
                    self.estimator.logs.push((
                        "warning".into(),
                        format!(
                            "Observation {} ignored due to failed {reason}",
                            event.service
                        ),
                    ));
                }
            }
        } else {
            self.initialized =
                state.all_checks(&[])? && self.sensor_checks([acceleration, gyroscope], clock());
        }
        if !state.topic("cameraOdometry")?.updated {
            return Ok(None);
        }
        let inputs = state.all_valid(&[])? && (0..3).all(|i| self.invalid[i] < self.thresholds[i]);
        let sensors = self.sensor_checks([acceleration, gyroscope], clock());
        let input_reasons = self.input_reasons(state);
        let sensor_reasons = self.sensor_reasons(clock());
        let diagnostic = if inputs && sensors {
            None
        } else {
            let now = clock();
            if now - self.last_print < 1. {
                None
            } else {
                self.last_print = now;
                let mut reasons = Vec::new();
                if !inputs {
                    reasons.push(format!("inputs_valid=False: {}", input_reasons.join("; ")));
                }
                if !sensors {
                    reasons.push(format!(
                        "sensors_valid=False: {}",
                        sensor_reasons.join("; ")
                    ));
                }
                Some(format!(
                    "[locationd] livePose invalid - {}",
                    reasons.join(" | ")
                ))
            }
        };
        Ok(Some(Output {
            pose: self.estimator.pose(sensors, inputs, self.initialized)?,
            diagnostic,
        }))
    }
    fn input_reasons(&self, state: &State) -> Vec<String> {
        let invalid: Vec<_> = state
            .topics()
            .iter()
            .filter(|topic| !topic.valid && !topic.ignores_valid())
            .map(|topic| topic.service.name)
            .collect();
        let mut reasons = Vec::new();
        if !invalid.is_empty() {
            reasons.push(format!("sm invalid: {}", invalid.join(", ")));
        }
        let observations: Vec<_> = (0..3)
            .filter(|i| self.invalid[*i] >= self.thresholds[*i])
            .map(|i| {
                format!(
                    "{}={:.2}/{:.2} ({})",
                    CRITICAL[i], self.invalid[i], self.thresholds[i], self.reasons[i]
                )
            })
            .collect();
        if !observations.is_empty() {
            reasons.push(format!("observation invalid: {}", observations.join(", ")));
        }
        reasons
    }
    fn sensor_reasons(&self, now: f64) -> Vec<String> {
        let mut reasons = Vec::new();
        for (i, name) in CRITICAL[..2].iter().enumerate() {
            if !self.sensor_alive[i] {
                reasons.push(if self.simulation {
                    format!("{name} not received this cycle")
                } else {
                    format!("{name} not alive age={:.3}s", now - self.sensor_received[i])
                });
            }
            if !self.sensor_valid[i] {
                reasons.push(format!("{name} msg.valid=False"));
            }
        }
        reasons
    }
}
