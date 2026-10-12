use openpilot_cluster::{
    models::RadarPoint,
    route_replay::corner::{adjust_point, decode_raw, sorted_points, RawObject, StableTracker},
};
use serde::Deserialize;
use std::io::{self, BufRead};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Query {
    Decode {
        t: f64,
        address: u32,
        data: Vec<u8>,
        speed: f64,
    },
    Track {
        objects: Vec<RawObject>,
        times: Vec<f64>,
        speed: f64,
    },
    Adjust {
        points: Vec<RadarPoint>,
        yaw: Option<f64>,
        gain: f64,
        offset: f64,
        width: f64,
    },
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let result = match serde_json::from_str::<Query>(&line?)? {
            Query::Decode {
                t,
                address,
                data,
                speed,
            } => {
                let objects = decode_raw(t, address, &data);
                serde_json::json!({"objects": objects, "valid": objects.iter().map(RawObject::valid).collect::<Vec<_>>(),
                    "points": objects.iter().map(|object|object.point(speed)).collect::<Vec<_>>()})
            }
            Query::Track {
                objects,
                times,
                speed,
            } => {
                let mut tracker = StableTracker::default();
                let mut frames = Vec::new();
                for object in objects {
                    tracker.update(&object)?;
                    frames.push(tracker.points_at(object.t, speed));
                }
                for time in times {
                    frames.push(tracker.points_at(time, speed));
                }
                serde_json::to_value(frames)?
            }
            Query::Adjust {
                points,
                yaw,
                gain,
                offset,
                width,
            } => serde_json::to_value(sorted_points(
                points
                    .iter()
                    .map(|point| adjust_point(point, yaw, gain, offset, width)),
            ))?,
        };
        println!("{}", serde_json::to_string(&result)?);
    }
    Ok(())
}
