//! Real producer child for isolated planner acquisition; caller owns namespace.
use openpilot_params::Params;
use openpilot_plannerd::signal::transport::ModeCache;
use openpilot_signal_observer::{
    entry::Entry,
    runtime::{self, Config, Error},
};
use serde_json::json;
use std::{
    env, fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
fn main() -> Result<(), Error> {
    let root = PathBuf::from(env::args_os().nth(1).ok_or("missing isolated root")?);
    if !env::var("OPENPILOT_PREFIX")?.starts_with("rust-probe-") {
        return Err("isolated worker requires probe namespace".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    let params = Params::for_runtime_at(&root.join("params"))?;
    let config = Config {
        directory: root.join("observer"),
        start: root.join("start.json"),
        prepare: root.join("prepare.json"),
        display: root.join("display.json"),
        frames: None,
        duration: Some(Duration::from_secs(120)),
    };
    let mut events = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("observer.jsonl"))?;
    let mut cache = ModeCache::default();
    Entry {
        config: &config,
        stop: &stop,
        params: &params,
        provider: None,
        child: None,
    }
    .run(
        &mut || {
            cache.read(
                runtime::monotonic_ns() as f64 / 1e9,
                || -> Result<i64, openpilot_params::Error> {
                    Ok(params
                        .get("SignalAssistMode")?
                        .map(|v| runtime::parameter_value(&v))
                        .unwrap_or(0))
                },
            )
        },
        &mut |name, fields| {
            writeln!(events, "{}", json!({"event":name,"fields":fields}))?;
            Ok(())
        },
        &mut |_| Err("classical producer must not start neural children".into()),
    )?;
    // A normal mode transition is a leaf return here; manager latch has its own
    // real-child/source gate. Never turn this fixture into a daemon selector.
    Ok(())
}
