use openpilot_lagd::{
    blocks::BlockAverage,
    correlation::{self, Correlator},
    estimate::{self, Delay},
    pose::{Calibrator, Pose},
    smoothing,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead};
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Input {
    Padding {
        values: Vec<usize>,
    },
    Peak {
        values: Vec<f64>,
        index: usize,
    },
    Smooth {
        values: Vec<f64>,
        mask: Vec<bool>,
        k: usize,
        sigma: f64,
    },
    Correlate {
        expected: Vec<f64>,
        actual: Vec<f64>,
        mask: Vec<bool>,
        n: usize,
    },
    Delay {
        expected: Vec<f64>,
        actual: Vec<f64>,
        mask: Vec<bool>,
        dt: f64,
        min: f64,
        max: f64,
    },
    Blocks {
        count: usize,
        size: usize,
        initial: f64,
        valid: i32,
        updates: Vec<f64>,
    },
    Pose {
        pose: Pose,
        rpy: [f64; 3],
        valid: bool,
    },
}
fn number(value: f64) -> Value {
    if value.is_nan() {
        json!("nan")
    } else if value.is_infinite() {
        json!(if value.is_sign_positive() {
            "inf"
        } else {
            "-inf"
        })
    } else {
        json!(value)
    }
}
fn numbers(values: &[f64]) -> Value {
    values.iter().copied().map(number).collect()
}
fn process(input: Input) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match input {
        Input::Padding { values } => json!(values
            .into_iter()
            .map(correlation::next_good_size)
            .collect::<Result<Vec<_>, _>>()?),
        Input::Peak { values, index } => number(estimate::parabolic(&values, index)?),
        Input::Smooth {
            values,
            mask,
            k,
            sigma,
        } => numbers(&smoothing::masked(&values, &mask, (k, sigma))?),
        Input::Correlate {
            expected,
            actual,
            mask,
            n,
        } => numbers(&Correlator::new(n)?.masked(&expected, &actual, &mask)?),
        Input::Delay {
            expected,
            actual,
            mask,
            dt,
            min,
            max,
        } => {
            let value =
                Delay::new(expected.len(), dt, (min, max))?.estimate(&expected, &actual, &mask)?;
            json!({"delay":number(value.delay),"correlation":number(value.correlation),"confidence":number(value.confidence),"peak":value.peak,"run":value.run,"width":value.width,"starts":value.starts,"ends":value.ends})
        }
        Input::Blocks {
            count,
            size,
            initial,
            valid,
            updates,
        } => {
            let mut blocks = BlockAverage::new(count, size, (initial, valid))?;
            let mut snapshots = Vec::new();
            for update in std::iter::once(None).chain(updates.into_iter().map(Some)) {
                if let Some(value) = update {
                    blocks.update(value)?;
                }
                let values = blocks.statistics()?;
                snapshots.push(json!({"values":numbers(&blocks.values),"block_idx":blocks.block_idx,"idx":blocks.idx,"valid_blocks":blocks.valid_blocks,"statistics":numbers(&[values.valid_mean,values.valid_std,values.current_mean,values.current_std])}));
            }
            json!(snapshots)
        }
        Input::Pose { pose, rpy, valid } => {
            let mut calibrator = Calibrator::default();
            calibrator.feed(rpy, valid);
            let pose = calibrator.pose(pose);
            let values = [
                pose.orientation,
                pose.velocity,
                pose.acceleration,
                pose.angular_velocity,
            ]
            .map(|value| json!({"xyz":numbers(&value.xyz),"std":numbers(&value.std)}));
            json!({"valid":calibrator.valid,"rotation":calibrator.rotation,"pose":values})
        }
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let input = serde_json::from_str(&line?)?;
        let output = match process(input) {
            Ok(value) => value,
            Err(error) => json!({"error":error.to_string()}),
        };
        println!("{output}");
    }
    Ok(())
}
