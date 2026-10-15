//! Initial lateral readiness from carrot-wip e8e152db; runtime failures never rearm it.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Service {
    pub seen: bool,
    pub valid: bool,
    pub alive: bool,
    /// The subscriber's all_checks result, including configured ignore policies.
    pub checks: bool,
}
impl Service {
    pub fn ready(self) -> bool {
        self.seen && self.checks
    }
    pub fn events_ready(self) -> bool {
        self.seen && self.valid && self.alive
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Services {
    pub boundary: Service,
    pub model: Service,
    pub live: Service,
    pub pose: Service,
    pub selfdrive: Service,
    pub events: Service,
    pub lane: Service,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct State {
    pub services: Services,
    /// inputsOK, posenetOK, sensorsOK.
    pub pose: [bool; 3],
    pub initializing: bool,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct LiveParameters {
    pub stiffness: f64,
    pub ratio: f64,
    pub offset: f64,
    pub roll: f64,
    pub valid: bool,
    pub sensor_valid: bool,
    pub posenet_valid: bool,
}
impl LiveParameters {
    pub fn ready(self, service: Service) -> bool {
        service.ready()
            && self.valid
            && self.sensor_valid
            && self.posenet_valid
            && [self.ratio, self.stiffness, self.offset, self.roll]
                .into_iter()
                .all(f64::is_finite)
            && self.ratio > 1.
            && self.stiffness > 0.
    }
    /// Nominal geometry is for inactive calculations only, never steering permission.
    pub fn for_inactive(self, service: Service, nominal_ratio: f64) -> Self {
        if self.ready(service) {
            self
        } else {
            Self {
                ratio: nominal_ratio,
                stiffness: 1.,
                ..Self::default()
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Car {
    pub can_valid: bool,
    pub fault_temporary: bool,
    pub fault_permanent: bool,
    /// vEgo, steeringAngleDeg, steeringRateDeg, steeringTorque.
    pub values: [f64; 4],
}
#[derive(Clone, Copy)]
pub struct Lane<'a> {
    pub use_lane_lines: bool,
    pub solution_valid: bool,
    pub psis: &'a [f64],
    pub curvatures: &'a [f64],
    pub distances: &'a [f64],
}
pub struct Inputs<'a> {
    pub state: &'a State,
    pub live: &'a LiveParameters,
    pub car: Car,
    pub desired_curvature: f64,
    pub lane: Lane<'a>,
}
impl Inputs<'_> {
    pub fn ready(&self) -> bool {
        let services = self.state.services;
        if ![
            services.model,
            services.live,
            services.pose,
            services.selfdrive,
        ]
        .into_iter()
        .all(Service::ready)
            || !services.events.events_ready()
            || !self.live.ready(services.live)
            || !self.car.can_valid
            || self.car.fault_temporary
            || self.car.fault_permanent
            || !self.car.values.into_iter().all(f64::is_finite)
            || self.state.initializing
            || !self.state.pose.into_iter().all(|valid| valid)
            || !self.desired_curvature.is_finite()
        {
            return false;
        }
        !self.lane.use_lane_lines
            || (services.lane.ready()
                && self.lane.solution_valid
                && [self.lane.psis, self.lane.curvatures, self.lane.distances]
                    .into_iter()
                    .all(|values| {
                        values.len() == 17 && values.iter().all(|value| value.is_finite())
                    }))
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Gate {
    pub ready: bool,
}
impl Gate {
    pub fn update(&mut self, inputs: Inputs<'_>) -> bool {
        if !self.ready {
            self.ready = inputs.state.services.boundary.ready() && inputs.ready();
        }
        self.ready
    }
}
