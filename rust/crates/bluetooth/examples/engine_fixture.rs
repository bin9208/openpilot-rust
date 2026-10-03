use indexmap::IndexMap;
use openpilot_bluetooth::{Address, Config, Engine, Event, Seconds, VehicleSnapshot};
use openpilot_logmessaged::JsonValue;
use serde::Deserialize;
use std::{
    error::Error,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Step {
    now: Seconds,
    config: String,
    learning: String,
    available: IndexMap<String, String>,
    errors: IndexMap<String, String>,
    snapshot: VehicleSnapshot,
    reads: IndexMap<String, Vec<Event>>,
}

fn capture(root: &Path) -> Result<IndexMap<&'static str, Option<String>>, io::Error> {
    ["cruise", "lane", "status"]
        .into_iter()
        .map(
            |name| match fs::read_to_string(root.join(format!("{name}.json"))) {
                Ok(value) => Ok((name, Some(value))),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((name, None)),
                Err(error) => Err(error),
            },
        )
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(std::env::args_os().nth(1).ok_or("root required")?);
    fs::create_dir_all(&root)?;
    let mut engine = Engine::new(&root, Config::default())?;
    let mut last_reload = 0.0;
    let mut last_status = 0.0;
    let mut last_time = Seconds(0.0);
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for line in io::stdin().lock().lines() {
        let step: Step = serde_json::from_str(&line?)?;
        last_time = step.now;
        engine.update(step.snapshot);
        let mut changes = Vec::new();
        if step.now.0 - last_reload >= 0.25 {
            last_reload = step.now.0;
            let available = step
                .available
                .into_iter()
                .map(|(path, address)| Ok((path, Address::parse(&address)?)))
                .collect::<Result<IndexMap<_, _>, openpilot_bluetooth::ConfigError>>()?;
            let reload = engine.reload(
                Config::parse(&step.config).unwrap_or_default(),
                JsonValue::parse(&step.learning).ok(),
                &available,
                step.now,
            );
            for path in reload.close {
                changes.push(format!("close:{path}"));
            }
            for (path, address) in reload.open {
                changes.push(format!("open:{path}"));
                if let Some(error) = step.errors.get(&path) {
                    engine.open_error(address, error.clone());
                } else {
                    engine.connect(&path, &address)?;
                }
            }
        }
        let paths: Vec<_> = engine.paths().map(str::to_owned).collect();
        for path in paths {
            match step.reads.get(&path) {
                None => engine.flush(&path, step.now, step.now)?,
                Some(events) if events.is_empty() => {
                    engine.disconnect(&path);
                    changes.push(format!("close:{path}"));
                }
                Some(events) => {
                    for event in events {
                        engine.event(&path, *event, step.now)?;
                    }
                    engine.flush(&path, step.now, step.now)?;
                }
            }
        }
        engine.prune(step.now)?;
        if step.now.0 - last_status >= 0.2 {
            last_status = step.now.0;
            engine.write_status(step.now)?;
        }
        serde_json::to_writer(
            &mut output,
            &serde_json::json!({"files":capture(&root)?,"changes":changes}),
        )?;
        writeln!(output)?;
    }
    engine.stopped(last_time)?;
    serde_json::to_writer(&mut output, &serde_json::json!({"stopped":capture(&root)?}))?;
    writeln!(output)?;
    Ok(())
}
