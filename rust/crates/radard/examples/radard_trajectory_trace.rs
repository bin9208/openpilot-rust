use openpilot_radard::{
    association::Matches,
    model::Model,
    path::{cache::Snapshot, Path},
    point::{Identity, Point},
    trajectory_cutin::{Detector, Frame},
    Lead,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io;

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
        sensitivity: i32,
    },
    Reset {
        owner: usize,
        cache: source_cache::Cursor,
    },
    Update {
        owner: usize,
        input: Value,
        cache: source_cache::Cursor,
    },
}

#[derive(Deserialize)]
struct SourceMatches {
    #[serde(default)]
    entries: Vec<(Identity, Point)>,
}

#[derive(Deserialize)]
struct Input {
    time_s: f64,
    v_ego: f64,
    model: Model,
    points: Vec<Point>,
    path: Vec<[f64; 2]>,
    yaw_rate_rad_s: f64,
    vision_required_front: bool,
    primary_lead: Option<Lead>,
    cross_sensor_matches: Option<SourceMatches>,
}

fn apply(
    action: Action,
    owners: &mut Vec<Detector>,
    archive: &Snapshot,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (owner, result) = match action {
        Action::Create { owner, sensitivity } => {
            if owner != owners.len() {
                return Err("trajectory owner order differs".into());
            }
            owners.push(Detector::new(sensitivity));
            return Ok(Value::Null);
        }
        Action::Reset { owner, cache } => {
            source_cache::restore(cache, archive)?;
            let owner = owners
                .get_mut(owner)
                .ok_or("trajectory reset owner absent")?;
            owner.reset();
            (owner, Value::Null)
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
                .ok_or("trajectory update owner absent")?;
            let path = Path::new(&input.path)?;
            let matches: Option<Matches> = input
                .cross_sensor_matches
                .map(|matches| matches.entries.into_iter().collect());
            let result = owner.update(Frame {
                time_s: input.time_s,
                v_ego: input.v_ego,
                points: &input.points,
                path: &path,
                model: &input.model,
                yaw_rate: input.yaw_rate_rad_s,
                vision_required_front: input.vision_required_front,
                primary: input.primary_lead.as_ref(),
                matches: matches.as_ref(),
            })?;
            let result = serde_json::to_value(result)?;
            (owner, result)
        }
    };
    Ok(json!({"result":result,"state":owner,"cache":source_cache::fingerprints()?}))
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
