use indexmap::IndexMap;
use openpilot_radard::{
    controller::{Controller, Input, Options},
    path::cache::Snapshot,
    point::{Identity, TrackId},
    predictor::{CutOutPredictor, Frame, Predictor},
    Error,
};
use serde::{Deserialize, Serialize};
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
// Serde's internally tagged content buffer cannot deserialize i128 TrackId.
// Parse those nested fixture values directly before calling the typed controller.
enum Action {
    Create {
        owner: usize,
        options: Options,
    },
    Update {
        owner: usize,
        input: Value,
        cache: source_cache::Cursor,
        mode: i32,
        predictor_override: Option<Value>,
        #[serde(default)]
        primary_reset: bool,
    },
}
#[derive(Deserialize, Serialize)]
struct Fixed {
    prediction: Prediction,
}
#[derive(Deserialize, Serialize)]
struct Prediction {
    source: String,
    track_id: TrackId,
    cut_out_probability: f64,
}
#[derive(Serialize)]
#[serde(untagged)]
enum FixturePredictor {
    Native(Predictor),
    Fixed(Fixed),
}
impl Default for FixturePredictor {
    fn default() -> Self {
        Self::Native(Predictor::default())
    }
}
impl CutOutPredictor for FixturePredictor {
    fn predict(&mut self, frame: Frame<'_>) -> Result<IndexMap<Identity, f64>, Error> {
        match self {
            Self::Native(value) => value.update(frame),
            Self::Fixed(value) => Ok(IndexMap::from([(
                Identity(value.prediction.source.clone(), value.prediction.track_id),
                value.prediction.cut_out_probability,
            )])),
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(io::stdin().lock())?;
    let mut owners: Vec<Controller<FixturePredictor>> = Vec::new();
    let mut results = Vec::new();
    for action in request.actions {
        match action {
            Action::Create { owner, options } => {
                if owner != owners.len() {
                    return Err("controller owner order differs".into());
                }
                owners.push(Controller::new(options));
                results.push(Value::Null);
            }
            Action::Update {
                owner,
                input,
                cache,
                mode,
                predictor_override,
                primary_reset,
            } => {
                let input: Input = serde_json::from_value(input)?;
                source_cache::restore(cache, &request.cache)?;
                let owner = owners.get_mut(owner).ok_or("controller owner absent")?;
                owner.enable_radar_tracks = mode;
                if primary_reset {
                    owner.primary_matcher.reset();
                }
                if let Some(fixed) = predictor_override {
                    owner.primary_cut_out_predictor =
                        FixturePredictor::Fixed(serde_json::from_value(fixed)?);
                }
                let output = owner.update(&input)?;
                results.push(
                    json!({"output":output,"state":owner,"cache":source_cache::fingerprints()?}),
                );
            }
        }
    }
    serde_json::to_writer(io::stdout().lock(), &results)?;
    Ok(())
}
