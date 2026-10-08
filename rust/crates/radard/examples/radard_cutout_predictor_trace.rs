use openpilot_radard::{
    path::{cache::Snapshot, Path, Projection},
    point::{Identity, Point, TrackId},
    predictor::{Frame, Predictor},
    scope::Scoped,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, io};

#[path = "selection/cache.rs"]
mod source_cache;

#[derive(Deserialize)]
struct Request {
    actions: Vec<Action>,
    cache: Snapshot,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Create {
        owner: usize,
    },
    Update {
        owner: usize,
        input: Value,
        cache: source_cache::Cursor,
    },
}

#[derive(Deserialize)]
struct Input {
    time_s: f64,
    v_ego: f64,
    points: Vec<Point>,
    path: Vec<[f64; 2]>,
    yaw_rate_rad_s: f64,
    scoped_points: Vec<(Point, f64, Projection)>,
    prediction_identities: HashSet<Identity>,
}

#[derive(Serialize)]
struct Prediction {
    track_id: TrackId,
    cut_out_probability: f64,
}

fn apply(
    action: Action,
    owners: &mut Vec<Predictor>,
    archive: &Snapshot,
) -> Result<Value, Box<dyn std::error::Error>> {
    match action {
        Action::Create { owner } => {
            if owner != owners.len() {
                return Err("cutout predictor owner order differs".into());
            }
            owners.push(Predictor::default());
            Ok(Value::Null)
        }
        Action::Update {
            owner,
            input,
            cache,
        } => {
            let input: Input = serde_json::from_value(input)?;
            source_cache::restore(cache, archive)?;
            let owner = owners
                .get_mut(owner)
                .ok_or("cutout predictor update owner absent")?;
            let path = Path::new(&input.path)?;
            let scoped: Vec<_> = input
                .scoped_points
                .iter()
                .map(|(point, distance, projection)| Scoped {
                    point,
                    distance: *distance,
                    projection: *projection,
                })
                .collect();
            let predictions = owner.update(Frame {
                time_s: input.time_s,
                v_ego: input.v_ego,
                points: &input.points,
                path: &path,
                yaw_rate: input.yaw_rate_rad_s,
                requested: &input.prediction_identities,
                scoped: Some(&scoped),
            })?;
            let result = if predictions.is_empty() {
                json!({})
            } else {
                json!({"entries":predictions.into_iter().map(|(identity,probability)|{
                let prediction=Prediction {track_id:identity.1,cut_out_probability:probability};(identity,prediction)
            }).collect::<Vec<_>>()})
            };
            Ok(json!({"result":result,"state":owner,"cache":source_cache::fingerprints()?}))
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(io::stdin().lock())?;
    let mut owners = Vec::new();
    let results: Vec<_> = request
        .actions
        .into_iter()
        .map(|action| apply(action, &mut owners, &request.cache))
        .collect::<Result<_, _>>()?;
    serde_json::to_writer(io::stdout().lock(), &results)?;
    Ok(())
}
