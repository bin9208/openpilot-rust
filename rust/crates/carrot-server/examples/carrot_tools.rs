use openpilot_carrot_server::{tools::jobs::Store, Value};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

#[path = "carrot_tools/application.rs"]
mod application;

#[path = "carrot_tools/http.rs"]
mod http;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = io::stdin();
    let mut lines = input.lock().lines();
    let config = Value::parse(&lines.next().ok_or("missing owned fixture config")??)?;
    if config.get("owned_root").truth() {
        drop(lines);
        return http::run(config).await;
    }
    let path = PathBuf::from(config.get("path").string()?);
    let now = config.get("now").float()?;
    let store = Store::with_clock(path, move || now);
    for line in lines {
        let step = Value::parse(&line?)?;
        let output = match step.get("operation").string()?.as_str() {
            "list" => store.snapshots(20)?,
            "get" => store.get(&step.get("id").string()?)?.unwrap_or(Value::Null),
            "clear" => Value::integer(store.clear()?),
            "prune" => Value::Bool(store.prune()?),
            "append" => {
                store.append(&step.get("id").string()?, step.get("text"))?;
                store.snapshots(20)?
            }
            "finish" => {
                store.finish(
                    &step.get("id").string()?,
                    step.get("ok").truth(),
                    step.get("result").clone(),
                )?;
                store.snapshots(20)?
            }
            "persist" => {
                store.persist();
                Value::Null
            }
            _ => return Err("unknown owned fixture operation".into()),
        };
        println!("{}", output.encode()?);
    }
    Ok(())
}
