use openpilot_plannerd::{
    lane_change_gap::{CreditInput, Input, Plan, Tracker},
    lead::Lead,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{self, Read},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Update {
        owner: usize,
        input: Input,
        scalars: [u64; 3],
    },
    Credit {
        plan: Plan,
        primary: Option<Lead>,
        horizons: Vec<f64>,
        scalars: [u64; 5],
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let actions: Vec<Action> = serde_json::from_str(&raw)?;
    let mut trackers = BTreeMap::<usize, Tracker>::new();
    let mut output = Vec::new();
    for action in actions {
        match action {
            Action::Update {
                owner,
                mut input,
                scalars,
            } => {
                [input.now, input.speed, input.yaw_rate] = scalars.map(f64::from_bits);
                let tracker = trackers.entry(owner).or_default();
                let plan = tracker.update(&input)?;
                output.push(json!({"plan":plan,"direction":tracker.direction()}));
            }
            Action::Credit {
                plan,
                primary,
                horizons,
                scalars,
            } => {
                let [speed, max_accel, follow, stop_distance, ratio] = scalars.map(f64::from_bits);
                let values = plan.credit(
                    primary.as_ref(),
                    &horizons,
                    CreditInput {
                        speed,
                        max_accel,
                        follow,
                        stop_distance,
                        ratio,
                    },
                )?;
                output.push(json!(values
                    .into_iter()
                    .map(f64::to_bits)
                    .collect::<Vec<_>>()));
            }
        }
    }
    serde_json::to_writer(io::stdout().lock(), &output)?;
    Ok(())
}
