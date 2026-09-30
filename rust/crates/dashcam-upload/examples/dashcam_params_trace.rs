use openpilot_dashcam_upload::metadata;
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Param {
        key: String,
        default: String,
    },
    Serial {
        environment: BTreeMap<String, String>,
        serial: String,
    },
    Metadata {
        environment: BTreeMap<String, String>,
        serial: String,
        repo: PathBuf,
    },
    Webhook {
        environment: BTreeMap<String, String>,
    },
}
#[derive(Deserialize)]
struct Request {
    marker: String,
    #[serde(default)]
    close: bool,
    #[serde(flatten)]
    operation: Operation,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let params = Params::open(std::path::Path::new(&args[1]), &args[2])?;
    let mut logger = Factory::new(args[3].clone())?.logger();
    println!("{}", json!({"initialized":true}));
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        if request.close {
            logger.close();
        }
        let value: Value = match request.operation {
            Operation::Param { key, default } => json!(metadata::param_text(
                Some(&params),
                &key,
                &default,
                &mut logger
            )),
            Operation::Serial {
                environment,
                serial,
            } => json!(metadata::device_serial(
                Some(&params),
                &environment,
                &serial,
                &mut logger
            )),
            Operation::Metadata {
                environment,
                serial,
                repo,
            } => {
                metadata::upload_metadata(Some(&params), &repo, &environment, &serial, &mut logger)
            }
            Operation::Webhook { environment } => json!(metadata::webhook_url(
                Some(&params),
                &environment,
                &mut logger
            )),
        };
        if !request.close {
            logger.emit(log_site!(), Record::text(Level::Debug, request.marker))?;
        }
        println!("{value}");
        io::stdout().flush()?;
    }
    Ok(())
}
