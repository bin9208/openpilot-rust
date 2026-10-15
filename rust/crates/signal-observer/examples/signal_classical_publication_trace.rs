//! Production record/publication phase regression; no camera/control access.
use openpilot_plannerd::signal::transport::Mode;
use openpilot_signal_observer::runtime::{self, Config, DIRECTORY};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, Read},
    path::Path,
};
#[derive(Deserialize)]
struct Case {
    modes: Vec<bool>,
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
    let root = env::temp_dir().join(format!("signal-classical-phases-{}", std::process::id()));
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
        let mut mode = || {
            let enabled = case.modes[calls.len()];
            calls.push(enabled);
            if enabled {
                Mode::Observe
            } else {
                Mode::Off
            }
        };
        let flag = runtime::observation_requested(&config, "start_guard_enabled", &mut mode)?;
        let record = json!({"timestamp_eof":10_000_000_000u64,"frame_id":20,"session":"phase","revalidated":true,"start_guard_transport":flag,"tracks":if case.malformed{Value::Null}else{json!([{"id":1,"box":[600.,200.,640.,220.],"state":"red","age":0.,"observations":3,"seen_red":true,"evidence":{"raw":"red"}}])}});
        let mut events = vec![];
        runtime::publish_classical(&config, &record, &mut mode, &mut |name, _| {
            events.push(name.to_owned());
            Ok(())
        })?;
        output.push(json!({"record_flag":flag,"calls":calls,"events":events,"payloads":{"start":read(&config.start)?,"prepare":read(&config.prepare)?,"display":read(&config.display)?}}));
    }
    fs::remove_dir_all(root)?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
