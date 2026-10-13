use openpilot_carrot_server::{
    auto_update::{self, AutoRebootCondition, ManagerReady, RebootSample},
    Error, Value,
};
use serde::Deserialize;
use std::io::Read;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Manager,
    Reboot,
    Target,
    ShortError,
}

fn delay(case: &Value, default: f64) -> Result<f64, Error> {
    Ok(if case.has("delay") {
        case.get("delay").float()?
    } else {
        default
    })
}

fn run(case: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let kind: Kind = serde_json::from_str(&case.get("kind").encode()?)?;
    let Value::Array(steps) = case.get("steps") else {
        return Err("steps must be an array".into());
    };
    let values = match kind {
        Kind::Manager => {
            let mut condition = ManagerReady::new(delay(case, auto_update::READY_DELAY)?);
            steps
                .iter()
                .map(|step| {
                    Ok(Value::Bool(condition.update(
                        step.get("now").float()?,
                        step.get("valid").truth(),
                    )))
                })
                .collect::<Result<Vec<_>, Error>>()?
        }
        Kind::Reboot => {
            let mut condition = AutoRebootCondition::new(
                &case.get("mode").string()?,
                delay(case, auto_update::DISENGAGED_DELAY)?,
            );
            steps
                .iter()
                .map(|step| {
                    Ok(Value::Bool(condition.update(&RebootSample {
                        now: step.get("now").float()?,
                        selfdrive_valid: step.get("selfdrive_valid").truth(),
                        engaged: step.get("engaged").truth(),
                        car_state_valid: step.get("car_state_valid").truth(),
                        gear_shifter: step.get("gear_shifter"),
                        device_state_valid: step.get("device_state_valid").truth(),
                        device_started: !step.has("device_started")
                            || step.get("device_started").truth(),
                    })))
                })
                .collect::<Result<Vec<_>, Error>>()?
        }
        Kind::Target => steps
            .iter()
            .map(|step| match auto_update::verified_update_target(step) {
                Ok((behind, head)) => {
                    Value::object([("result", Value::Array(vec![Value::integer(behind), head]))])
                }
                Err(Error::Json(error)) => Value::object([
                    ("exception", Value::text(error.kind)),
                    ("message", Value::text(&error.to_string())),
                ]),
                Err(error) => Value::object([
                    ("exception", Value::text("NativeError")),
                    ("message", Value::text(&error.to_string())),
                ]),
            })
            .collect(),
        Kind::ShortError => steps
            .iter()
            .map(|step| {
                let Value::Text(fallback) = step.get("fallback") else {
                    return Err(Error::Source("fallback must be a string".into()));
                };
                auto_update::short_error(step.get("output"), fallback)
            })
            .collect::<Result<Vec<_>, Error>>()?,
    };
    Ok(Value::Array(values))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    println!("{}", run(&Value::parse(&raw)?)?.encode()?);
    Ok(())
}
