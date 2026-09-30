use openpilot_calibrationd::{wire, Calibrator, Error, Limits, Odometry, Seed, Update};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct SeedInput {
    rpy: Vec<String>,
    valid_blocks: i32,
    wide: Vec<String>,
    height: Vec<String>,
}

#[derive(Deserialize)]
struct Input {
    trans: Vec<String>,
    rot: Vec<String>,
    trans_std: Vec<String>,
    wide: Vec<String>,
    road: Vec<String>,
    road_std: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Command {
    Reset {
        mici: bool,
        seed: Option<SeedInput>,
        saved: Option<Vec<u8>>,
        not_car: bool,
    },
    Update {
        input: Input,
        v_ego: String,
        trim: String,
        timestamp: u64,
        valid: bool,
    },
    Snapshot {
        timestamp: u64,
        valid: bool,
    },
}

fn numbers(values: Vec<String>) -> Result<Vec<f64>, Error> {
    values
        .iter()
        .map(|value| {
            value
                .parse()
                .map_err(|_| Error::Contract("invalid probe number"))
        })
        .collect()
}

fn strings(values: &[f64]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

fn snapshot(calibrator: &Calibrator, timestamp: u64, valid: bool) -> Result<Value, Error> {
    Ok(json!({
        "rpy": strings(&calibrator.rpy), "wide": strings(&calibrator.wide),
        "height": calibrator.height.to_string(), "spread": strings(&calibrator.spread),
        "old_rpy": strings(&calibrator.old_rpy), "old_weight": calibrator.old_weight.to_string(),
        "idx": calibrator.idx, "block_idx": calibrator.block_idx,
        "valid_blocks": calibrator.valid_blocks, "valid_indices": calibrator.valid_indices(),
        "status": format!("{:?}", calibrator.status).to_lowercase(),
        "packet": wire::encode(calibrator, timestamp, valid)?,
    }))
}

fn execute(command: Command, current: &mut Option<Calibrator>) -> Result<Value, Error> {
    match command {
        Command::Reset {
            mici,
            seed,
            saved,
            not_car,
        } => {
            *current = None;
            let (seed, saved_error) = match saved {
                Some(bytes) => wire::saved(&bytes),
                None => (
                    match seed {
                        Some(input) => Seed {
                            rpy: numbers(input.rpy)?,
                            valid_blocks: input.valid_blocks,
                            wide: numbers(input.wide)?,
                            height: numbers(input.height)?,
                        },
                        None => Seed::default(),
                    },
                    None,
                ),
            };
            let mut calibrator = Calibrator::new(
                if mici {
                    Limits::mici()
                } else {
                    Limits::standard()
                },
                seed,
            )?;
            calibrator.not_car = not_car;
            let mut response = snapshot(&calibrator, 0, true)?;
            response["saved_error"] = json!(saved_error.is_some());
            *current = Some(calibrator);
            Ok(response)
        }
        Command::Update {
            input,
            v_ego,
            trim,
            timestamp,
            valid,
        } => {
            let calibrator = current.as_mut().ok_or(Error::Contract("reset required"))?;
            calibrator.v_ego = v_ego
                .parse()
                .map_err(|_| Error::Contract("invalid probe ego speed"))?;
            let trim = trim
                .parse()
                .map_err(|_| Error::Contract("invalid probe trim"))?;
            let update = if calibrator.frozen(trim) {
                Update {
                    rpy: None,
                    persist: false,
                }
            } else {
                calibrator.update(&Odometry {
                    trans: numbers(input.trans)?,
                    rot: numbers(input.rot)?,
                    trans_std: numbers(input.trans_std)?,
                    wide: numbers(input.wide)?,
                    road: numbers(input.road)?,
                    road_std: numbers(input.road_std)?,
                })?
            };
            let mut response = snapshot(calibrator, timestamp, valid)?;
            response["accepted"] = json!(update.rpy.map(|values| strings(&values)));
            response["persist"] = json!(update.persist);
            Ok(response)
        }
        Command::Snapshot { timestamp, valid } => snapshot(
            current.as_ref().ok_or(Error::Contract("reset required"))?,
            timestamp,
            valid,
        ),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut calibrator = None;
    for line in io::stdin().lock().lines() {
        let command = serde_json::from_str(&line?)?;
        let result = match execute(command, &mut calibrator) {
            Ok(value) => value,
            Err(error) => json!({"error": error.to_string()}),
        };
        println!("{}", serde_json::to_string(&result)?);
    }
    Ok(())
}
