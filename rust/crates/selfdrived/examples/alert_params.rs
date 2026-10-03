use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_params::Params;
use openpilot_selfdrived::callbacks::{AlertParams, Error, NativeParams};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Text,
    Integer,
    Boolean,
}

#[derive(Deserialize)]
struct Request {
    operation: Operation,
    key: String,
    marker: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [root, prefix, endpoint] = arguments.as_slice() else {
        return Err("expected root, prefix and logging endpoint".into());
    };
    let params = Params::open(Path::new(root), prefix)?;
    let mut logger = Factory::new(endpoint.clone())?.logger();
    println!("ready");
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let mut adapter = NativeParams {
            params: &params,
            logger: &mut logger,
        };
        let value = match request.operation {
            Operation::Text => adapter.text(&request.key).map(|value| json!(value)),
            Operation::Integer => adapter.integer(&request.key).map(|value| json!(value)),
            Operation::Boolean => adapter.boolean(&request.key).map(|value| json!(value)),
        };
        let result = match value {
            Ok(value) => json!({"value":value}),
            Err(Error::IntegerParameter { .. }) => json!({"error":"fatal_integer"}),
            Err(
                Error::Parameter(openpilot_params::Error::UnknownKey(_))
                | Error::TextParameter(openpilot_params_typed::Error::Params(
                    openpilot_params::Error::UnknownKey(_),
                )),
            ) => json!({"error":"unknown_key"}),
            Err(error) => return Err(error.into()),
        };
        logger.emit(log_site!(), Record::text(Level::Debug, request.marker))?;
        println!("{result}");
        io::stdout().flush()?;
    }
    Ok(())
}
