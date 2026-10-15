use openpilot_control_policy::lateral_readiness::{Car, Gate, Inputs, Lane, LiveParameters, State};
use serde::Deserialize;
use serde_json::json;
use std::{
    error::Error,
    io::{self, BufRead},
};
#[derive(Deserialize)]
struct Plan {
    use_lane_lines: bool,
    solution_valid: bool,
    psis: Vec<f64>,
    curvatures: Vec<f64>,
    distances: Vec<f64>,
}
#[derive(Deserialize)]
struct Request {
    reset: bool,
    state: State,
    live: LiveParameters,
    car: Car,
    lane: Plan,
    desired_curvature: f64,
    nominal: f64,
    #[serde(default)]
    nonfinite: Option<String>,
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut gate = Gate::default();
    for line in io::stdin().lock().lines() {
        let mut r: Request = serde_json::from_str(&line?)?;
        if r.reset {
            gate = Gate::default();
        }
        if let Some(field) = r.nonfinite.as_deref() {
            match field {
                "live.ratio" => r.live.ratio = f64::NAN,
                "live.stiffness" => r.live.stiffness = f64::INFINITY,
                "live.offset" => r.live.offset = f64::NAN,
                "live.roll" => r.live.roll = f64::NEG_INFINITY,
                "car.speed" => r.car.values[0] = f64::NAN,
                "car.angle" => r.car.values[1] = f64::INFINITY,
                "car.rate" => r.car.values[2] = f64::NAN,
                "car.torque" => r.car.values[3] = f64::INFINITY,
                "model" => r.desired_curvature = f64::NAN,
                "lane.psis" => r.lane.psis[8] = f64::NAN,
                "lane.curvatures" => r.lane.curvatures[8] = f64::INFINITY,
                "lane.distances" => r.lane.distances[8] = f64::NAN,
                _ => return Err("unknown nonfinite fixture".into()),
            }
        }
        let live_ready = r.live.ready(r.state.services.live);
        let parameters = r.live.for_inactive(r.state.services.live, r.nominal);
        let ready = gate.update(Inputs {
            state: &r.state,
            live: &r.live,
            car: r.car,
            desired_curvature: r.desired_curvature,
            lane: Lane {
                use_lane_lines: r.lane.use_lane_lines,
                solution_valid: r.lane.solution_valid,
                psis: &r.lane.psis,
                curvatures: &r.lane.curvatures,
                distances: &r.lane.distances,
            },
        });
        println!(
            "{}",
            json!({"ready":ready,"live_ready":live_ready,"geometry":[parameters.ratio,parameters.stiffness,parameters.offset,parameters.roll]})
        );
    }
    Ok(())
}
