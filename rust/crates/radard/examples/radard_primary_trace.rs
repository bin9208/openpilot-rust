use openpilot_radard::{
    model::{Model, VisionLead},
    path::{cache::Snapshot, Path},
    point::Point,
    primary::{Frame, Matcher, VisionMatch},
};
use serde::Deserialize;
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
    Reset {
        owner: usize,
        cache: source_cache::Cursor,
    },
    Update {
        owner: usize,
        input: Value,
        cache: source_cache::Cursor,
    },
    Handoff {
        owner: usize,
        input: Value,
        cache: source_cache::Cursor,
    },
}

#[derive(Deserialize)]
struct Input {
    model: Model,
    points: Vec<Point>,
    path: Vec<[f64; 2]>,
    time_s: Option<f64>,
    stationary_points: Option<Vec<Point>>,
    prefer_corner_stationary: bool,
    prefer_primary_stationary: bool,
    yaw_rate_rad_s: f64,
    allowed_output_sources: Option<HashSet<String>>,
}

#[derive(Deserialize)]
struct Handoff {
    stationary: Option<VisionMatch>,
    moving: Option<VisionMatch>,
    vision: Option<VisionLead>,
    time_s: Option<f64>,
}

fn apply(
    action: Action,
    owners: &mut Vec<Matcher>,
    archive: &Snapshot,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (owner, result) = match action {
        Action::Create { owner } => {
            if owner != owners.len() {
                return Err("matcher owner order differs".into());
            }
            owners.push(Matcher::default());
            return Ok(Value::Null);
        }
        Action::Reset { owner, cache } => {
            source_cache::restore(cache, archive)?;
            let owner = owners.get_mut(owner).ok_or("matcher reset owner absent")?;
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
            let owner = owners.get_mut(owner).ok_or("matcher update owner absent")?;
            let path = Path::new(&input.path)?;
            let frame = Frame {
                vision: input.model.primary_vision(),
                points: &input.points,
                path: &path,
                time: input.time_s,
                prefer_corner: input.prefer_corner_stationary,
                prefer_primary: input.prefer_primary_stationary,
                yaw_rate: input.yaw_rate_rad_s,
                allowed_output_sources: input.allowed_output_sources.as_ref(),
            };
            let result = owner.update(&frame, input.stationary_points.as_deref());
            (owner, serde_json::to_value(result)?)
        }
        Action::Handoff {
            owner,
            input,
            cache,
        } => {
            let input: Handoff = serde_json::from_value(input)?;
            source_cache::restore(cache, archive)?;
            let owner = owners
                .get_mut(owner)
                .ok_or("matcher handoff owner absent")?;
            let result = owner.closer_handoff_ready(
                input.stationary.as_ref(),
                input.moving.as_ref(),
                input.vision,
                input.time_s,
            );
            (owner, Value::Bool(result))
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
