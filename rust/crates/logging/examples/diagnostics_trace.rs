use openpilot_logging::{diagnostics::Diagnostics, Fields, Number, Value};
use serde::Deserialize;
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Setup {
    component: String,
    interval: f64,
    started: f64,
    scheduler: Option<[u64; 3]>,
    enabled: Option<bool>,
    pid: u32,
}
#[derive(Deserialize)]
struct Sample {
    name: String,
    value: String,
    integer: bool,
}
#[derive(Deserialize)]
struct Step {
    now: f64,
    scheduler: Option<[u64; 3]>,
    values: Vec<Sample>,
    context: Vec<(String, Value)>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let setup: Setup = serde_json::from_str(&lines.next().ok_or("missing setup")??)?;
    let mut diagnostics = Diagnostics::new(
        &setup.component,
        setup.interval,
        setup.started,
        setup.scheduler,
        setup.enabled,
        setup.pid,
    );
    for line in lines {
        let step: Step = serde_json::from_str(&line?)?;
        let values = step
            .values
            .into_iter()
            .map(|sample| {
                let value = if sample.integer {
                    Number::Integer(sample.value.parse()?)
                } else {
                    Number::Float(f64::from_bits(u64::from_str_radix(&sample.value, 16)?))
                };
                Ok::<_, Box<dyn std::error::Error>>((sample.name, value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let context: Fields = step.context.into_iter().collect();
        let event = diagnostics.record_with(values, context, || step.now, || step.scheduler)?;
        println!(
            "{}",
            event
                .map(|event| event.to_json())
                .transpose()?
                .unwrap_or("null".into())
        );
    }
    Ok(())
}
