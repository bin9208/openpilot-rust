use openpilot_carrot_server::{
    git_state::{did_pull_update, Store, Time},
    Error, Value,
};
use std::io::{BufRead, Write};

fn required_text(value: &Value) -> Result<String, Error> {
    Ok(value.string()?)
}

fn operate(store: &Store, step: &Value) -> Result<Value, Error> {
    let clock = Time {
        seconds: step.get("seconds").clone(),
        nanoseconds: step.get("nanoseconds").clone(),
    };
    let operation = required_text(step.get("operation"))?;
    match operation.as_str() {
        "read" => Ok(store.read()),
        "write" => Ok(Value::Bool(store.write(step.get("data")))),
        "meta" => Ok(store
            .custom_meta(&required_text(step.get("name"))?)
            .unwrap_or(Value::Null)),
        "pull_time" => {
            store.write_pull_time(step.get("timestamp"), &clock)?;
            Ok(Value::Null)
        }
        "auto_read" => Ok(store.auto_update()),
        "event" => store.write_event(step.get("status"), step.get("fields"), &clock),
        "did_pull" => Ok(Value::Bool(did_pull_update(step.get("output"))?)),
        _ => Err(Error::Source("unknown owned Git state operation".into())),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lines = std::io::stdin().lock().lines();
    let line = lines
        .next()
        .ok_or("owned Git state configuration missing")??;
    let config = Value::parse(&line)?;
    let store = Store::new(required_text(config.get("directory"))?.into());
    for line in lines {
        let step = Value::parse(&line?)?;
        let output = match operate(&store, &step) {
            Ok(output) => Value::object([("result", output)]),
            Err(Error::Json(error)) => Value::object([
                ("exception", Value::text(error.kind)),
                ("message", error.message_value()),
            ]),
            Err(
                error @ (Error::Source(_)
                | Error::Io(_)
                | Error::UnknownCharset(_)
                | Error::Request(_)
                | Error::Params(_)),
            ) => Value::object([
                ("exception", Value::text("NativeError")),
                ("message", Value::text(&error.to_string())),
            ]),
        };
        println!("{}", output.encode()?);
        std::io::stdout().flush()?;
    }
    Ok(())
}
