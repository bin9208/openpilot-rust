//! Production record/publication phase regression; no camera/control access.
use openpilot_signal_observer::{
    neural_runtime,
    runtime::{Config, Error, DIRECTORY},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, Read},
    path::Path,
};
#[derive(Deserialize)]
struct Case {
    modes: Vec<Value>,
    malformed: bool,
}
fn read(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(if path.exists() {
        serde_json::from_slice(&fs::read(path)?)?
    } else {
        Value::Null
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let root = env::temp_dir().join(format!("signal-neural-phases-{}", std::process::id()));
    fs::create_dir(&root)?;
    let mut output = vec![];
    for (index, case) in cases.into_iter().enumerate() {
        let directory = root.join(index.to_string());
        fs::create_dir(&directory)?;
        let config = Config {
            directory: DIRECTORY.into(),
            start: directory.join("start.json"),
            prepare: directory.join("prepare.json"),
            display: directory.join("display.json"),
            ..Config::default()
        };
        let mut calls = vec![];
        let mut flag = |_name: &str| -> Result<bool, Error> {
            let value = case.modes[calls.len()].clone();
            calls.push(value.clone());
            value
                .as_bool()
                .ok_or_else(|| "fixture flag decode failure".into())
        };
        let record_flag = flag("start_guard_enabled");
        let value = record_flag.as_ref().ok().copied();
        let record = json!({"timestamp_eof":10_000_000_000u64,"frame_id":20,"session":"phase","revalidated":true,"start_guard_transport":value,"tracks":if case.malformed{Value::Null}else{json!([{"id":1,"box":[600.,200.,640.,220.],"state":"red","age":0.,"observations":3,"seen_red":true,"evidence":{"raw":"red"}}])}});
        let mut events = vec![];
        let error = match record_flag {
            Err(_) => true,
            Ok(_) => neural_runtime::publish(&config, &record, &mut flag, &mut |name, _| {
                events.push(name.to_owned());
                Ok(())
            })
            .is_err(),
        };
        output.push(json!({"record_flag":value,"calls":calls,"events":events,"error":error,"payloads":{"start":read(&config.start)?,"display":read(&config.display)?}}));
    }
    fs::remove_dir_all(root)?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
