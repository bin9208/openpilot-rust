use openpilot_plannerd::{
    coasting::{CoastingInput, CruiseCoastingPlan},
    lane_departure::{LaneDeparture, LaneDepartureInput, Warning},
    lead_dynamics::{AccelerationSample, LeadAccelTau},
    path_geometry::yaw_from_path,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    departure: Vec<LaneDepartureInput>,
    coasting: Vec<CoastingInput>,
    lead_tau: Vec<AccelerationSample>,
    geometry: Vec<GeometryInput>,
}

#[derive(Serialize)]
struct Response {
    departure: Vec<Warning>,
    coasting: Vec<[f64; 2]>,
    lead_tau: Vec<f64>,
    geometry: Vec<[Vec<f64>; 2]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeometryInput {
    path: Vec<[f64; 3]>,
    speeds: Vec<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let mut departure = LaneDeparture::default();
    let mut coasting = CruiseCoastingPlan::default();
    let mut lead_tau = LeadAccelTau::new(1.5);
    let mut geometry = Vec::new();
    for frame in request.geometry {
        let path: [[f64; 3]; 33] = frame
            .path
            .try_into()
            .map_err(|_| "geometry path needs 33 points")?;
        let speeds: [f64; 33] = frame
            .speeds
            .try_into()
            .map_err(|_| "geometry speed needs 33 points")?;
        let output = yaw_from_path(&path, &speeds)?;
        geometry.push([output.yaw.to_vec(), output.rate.to_vec()]);
    }
    let response = Response {
        geometry,
        lead_tau: request
            .lead_tau
            .into_iter()
            .map(|sample| lead_tau.update(sample))
            .collect(),
        departure: request
            .departure
            .iter()
            .map(|frame| departure.update(frame))
            .collect(),
        coasting: request
            .coasting
            .iter()
            .map(|frame| [coasting.update(frame), coasting.stable_time()])
            .collect(),
    };
    serde_json::to_writer(io::stdout().lock(), &response)?;
    Ok(())
}
