use openpilot_radard::{
    association::Associator,
    path::Path,
    point::{velocity_in_ego_frame, Point},
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Deserialize)]
struct Request {
    path: Vec<[f64; 2]>,
    queries: Vec<[f64; 2]>,
    samples: Vec<[f64; 2]>,
    steps: Vec<Vec<Point>>,
    yaw: f64,
    norms: Vec<Vec<u64>>,
}

#[derive(Serialize)]
struct Response {
    projections: Vec<openpilot_radard::path::Projection>,
    samples: Vec<[f64; 2]>,
    steps: Vec<serde_json::Value>,
    norms: Vec<u64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let requests: Vec<Request> = serde_json::from_reader(io::stdin().lock())?;
    let mut responses = Vec::new();
    for request in requests {
        let path = Path::new(&request.path)?;
        let mut associator = Associator::default();
        let mut steps = Vec::new();
        for points in request.steps {
            let matches = associator.update(&points);
            steps.push(serde_json::json!({
                "matches": matches.iter().collect::<Vec<_>>(),
                "pairs": associator.pairs.iter().collect::<Vec<_>>(),
                "velocities": points.iter().map(|point| velocity_in_ego_frame(point, request.yaw)).collect::<Vec<_>>(),
            }));
        }
        responses.push(Response {
            projections: request
                .queries
                .iter()
                .map(|[x, y]| path.project(*x, *y))
                .collect(),
            samples: request
                .samples
                .iter()
                .map(|[distance, offset]| path.at(*distance, *offset))
                .collect(),
            steps,
            norms: request
                .norms
                .iter()
                .map(|values| {
                    let values: Vec<_> =
                        values.iter().map(|value| f64::from_bits(*value)).collect();
                    openpilot_radard::math::norm(&values).to_bits()
                })
                .collect(),
        });
    }
    serde_json::to_writer(io::stdout().lock(), &responses)?;
    Ok(())
}
