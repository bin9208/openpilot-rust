use openpilot_radard::{
    model::VisionLead,
    path::Path,
    point::Point,
    selection::{Candidate, Identity, LeadTwoTracker, PrimaryHandoff, StationaryShadow},
    trajectory_cutout::{Input as CutoutInput, Tracker as CutoutTracker},
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
    cache: openpilot_radard::path::cache::Snapshot,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    LeadTwo,
    Shadow,
    Handoff,
    Cutout,
}

enum Owner {
    LeadTwo(LeadTwoTracker),
    Shadow(StationaryShadow),
    Handoff(PrimaryHandoff),
    Cutout(CutoutTracker),
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Create {
        owner: usize,
        kind: Kind,
    },
    Reset {
        owner: usize,
    },
    LeadTwo {
        owner: usize,
        input: Value,
    },
    Shadow {
        owner: usize,
        input: Value,
    },
    Handoff {
        owner: usize,
        input: Value,
    },
    Cutout {
        owner: usize,
        input: Value,
        cache: source_cache::Cursor,
    },
}

#[derive(Deserialize)]
struct SelectionInput {
    time_s: f64,
    primary: Option<Lead>,
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct ShadowInput {
    time_s: f64,
    primary: Option<Lead>,
    primary_cut_out_probability: f64,
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct HandoffInput {
    time_s: f64,
    primary: Option<Lead>,
    candidates: Vec<Candidate>,
    active_identity: Option<Identity>,
}

#[derive(Deserialize)]
struct TrajectoryInput {
    time_s: f64,
    point: Option<Point>,
    lateral: Option<Point>,
    vision: Option<VisionLead>,
    path: Vec<[f64; 2]>,
    v_ego: f64,
    yaw_rate: f64,
}

fn apply(
    action: Action,
    owners: &mut Vec<Owner>,
    archive: &openpilot_radard::path::cache::Snapshot,
) -> Result<Value, Box<dyn std::error::Error>> {
    match action {
        Action::Create { owner, kind } => {
            if owner != owners.len() {
                return Err("owner creation order differs".into());
            }
            owners.push(match kind {
                Kind::LeadTwo => Owner::LeadTwo(LeadTwoTracker::default()),
                Kind::Shadow => Owner::Shadow(StationaryShadow::default()),
                Kind::Handoff => Owner::Handoff(PrimaryHandoff::default()),
                Kind::Cutout => Owner::Cutout(CutoutTracker::default()),
            });
            Ok(Value::Null)
        }
        Action::Reset { owner } => {
            match owners.get_mut(owner).ok_or("reset owner missing")? {
                Owner::LeadTwo(owner) => owner.reset(),
                Owner::Shadow(owner) => owner.reset(),
                Owner::Handoff(owner) => owner.reset(),
                Owner::Cutout(owner) => owner.reset(),
            }
            Ok(Value::Null)
        }
        Action::LeadTwo { owner, input } => {
            let input: SelectionInput = serde_json::from_value(input)?;
            let Some(Owner::LeadTwo(owner)) = owners.get_mut(owner) else {
                return Err("lead-two owner missing".into());
            };
            let result = owner.update(input.time_s, input.primary.as_ref(), &input.candidates);
            Ok(json!({"result": result, "state": owner}))
        }
        Action::Shadow { owner, input } => {
            let input: ShadowInput = serde_json::from_value(input)?;
            let Some(Owner::Shadow(owner)) = owners.get_mut(owner) else {
                return Err("shadow owner missing".into());
            };
            let result = owner.update(
                input.time_s,
                input.primary.as_ref(),
                input.primary_cut_out_probability,
                &input.candidates,
            );
            Ok(json!({"result": result, "state": owner}))
        }
        Action::Handoff { owner, input } => {
            let input: HandoffInput = serde_json::from_value(input)?;
            let Some(Owner::Handoff(owner)) = owners.get_mut(owner) else {
                return Err("handoff owner missing".into());
            };
            let result = owner.update(
                input.time_s,
                input.primary.as_ref(),
                &input.candidates,
                input.active_identity.as_ref(),
            );
            Ok(json!({"result": result, "state": owner}))
        }
        Action::Cutout {
            owner,
            input,
            cache,
        } => {
            let input: TrajectoryInput = serde_json::from_value(input)?;
            source_cache::restore(cache, archive)?;
            let Some(Owner::Cutout(owner)) = owners.get_mut(owner) else {
                return Err("cutout owner missing".into());
            };
            let path = if input.path.is_empty() {
                None
            } else {
                Some(Path::new(&input.path)?)
            };
            let result = owner.update(CutoutInput {
                time_s: input.time_s,
                point: input.point.as_ref(),
                lateral: input.lateral.as_ref(),
                vision: input.vision.as_ref(),
                path: path.as_ref(),
                v_ego: input.v_ego,
                yaw_rate: input.yaw_rate,
            })?;
            Ok(json!({"result": result, "state": owner, "cache": source_cache::fingerprints()?}))
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
