use openpilot_ui_application::onroad::alert::{policy::Selection, Alert, Input, Policy};
use serde::Deserialize;
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    compact: bool,
    inputs: Vec<Input>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for case in cases {
        let policy = Policy::new(case.compact, str::to_owned);
        let mut previous: Option<Alert> = None;
        let mut rows = Vec::new();
        for input in case.inputs {
            let current = match policy.select(&input) {
                Selection::None => None,
                Selection::Current(alert) => {
                    if case.compact {
                        previous = Some(alert.clone());
                    }
                    Some(alert)
                }
                Selection::Fallback(alert) => Some(alert),
            };
            rows.push(serde_json::json!({"current":current,"previous":previous}));
        }
        output.push(rows);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
