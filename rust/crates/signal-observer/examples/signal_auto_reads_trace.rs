use openpilot_plannerd::signal::transport::Mode;
use openpilot_signal_observer::backend;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Read},
};
#[derive(Deserialize)]
struct Case {
    mode: i64,
    values: BTreeMap<String, bool>,
    failure: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = cases
        .into_iter()
        .map(|case| {
            let mut calls = vec![json!(["int", "SignalAssistMode"])];
            let mode =
                Mode::from_parameter(if case.failure.as_deref() == Some("SignalAssistMode") {
                    0
                } else {
                    case.mode
                });
            let result = backend::automatic_read(mode, |key| {
                calls.push(json!(["bool", key]));
                if case.failure.as_deref() == Some(key) {
                    Err(format!("read:{key}"))
                } else {
                    Ok(case.values[key])
                }
            });
            let mut record = match result {
                Ok(backend) => json!({"backend":backend}),
                Err(error) => json!({"error":error}),
            };
            record["calls"] = Value::Array(calls);
            record
        })
        .collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
