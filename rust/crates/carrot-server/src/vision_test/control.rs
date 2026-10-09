use super::{status, storage, Config};
use crate::{json_fields::set, youtube_test::process, Error, Value};
use openpilot_process_supervision::CapturedCommand;
use std::{fs, process::Stdio, time::Duration};

pub(super) async fn start(config: &Config) -> Result<i32, Error> {
    if status::get(config)?.get("runner_alive").truth() {
        println!("[vision_test] already running");
        return status::print(config);
    }
    if !config.boolean("IsOffroad")? {
        eprintln!("[vision_test] refused: device is not offroad");
        return Ok(1);
    }
    for (name, spec) in &config.children {
        let pids = process::matching(&spec.pattern);
        if !pids.is_empty() {
            eprintln!(
                "[vision_test] refused: {name} already running pid={}",
                pids.iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            return Ok(1);
        }
    }
    fs::write(&config.log, "").map_err(|error| crate::state::io_error(error, &config.log))?;
    let log = fs::OpenOptions::new()
        .append(true)
        .open(&config.log)
        .map_err(|error| crate::state::io_error(error, &config.log))?;
    let mut argv = vec![config.runner.path.as_os_str().to_owned()];
    argv.extend(config.runner.args.iter().cloned());
    argv.push("_run".into());
    let mut child = CapturedCommand {
        launcher: config.launcher.clone(),
        cwd: config.repository.clone(),
        argv,
    }
    .spawn_session_redirected(Stdio::from(log.try_clone()?), Stdio::from(log))
    .map_err(|error| match error {
        openpilot_process_supervision::Error::Io(error) => {
            crate::state::io_error(error, &config.runner.path)
        }
        error => Error::Source(error.to_string()),
    })?;
    println!("[vision_test] starting runner pid={}", child.process.id());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let status = status::get(config)?;
        if status.get("status").text_eq("running") {
            println!("[vision_test] ready");
            return status::print(config);
        }
        if status.get("status").text_eq("error") {
            eprintln!(
                "[vision_test] failed: {}",
                if status.get("error").truth() {
                    storage::text(status.get("error"))
                } else {
                    "unknown error".into()
                }
            );
            return Ok(1);
        }
        if child.process.try_wait()?.is_some() {
            eprintln!("[vision_test] runner exited during startup");
            return Ok(1);
        }
    }
    println!("[vision_test] startup is still in progress; run 'carrot vision status'");
    Ok(1)
}
pub(super) async fn children(config: &Config, state: &Value) -> Result<(), Error> {
    for (name, spec) in config.children.iter().rev() {
        process::terminate(
            status::pid(state.get("children").get(name))?,
            &spec.pattern,
            Duration::from_secs(3),
        )
        .await?;
    }
    Ok(())
}
pub(super) async fn stop(config: &Config) -> Result<i32, Error> {
    let state = storage::read(config);
    let pid = status::pid(state.get("runner_pid"))?;
    if process::alive(pid, &config.runner.pattern) {
        println!("[vision_test] stopping runner pid={pid}");
        process::terminate(pid, &config.runner.pattern, Duration::from_secs(15)).await?;
    } else if state.truth() {
        println!("[vision_test] runner is not active; cleaning stale state");
        children(config, &state).await?;
        config.snapshot(false)?;
    } else {
        println!("[vision_test] runner is not active");
    }
    let mut state = storage::read(config);
    if state.truth() {
        set(&mut state, "status", Value::text("stopped"))?;
        set(&mut state, "children", Value::Object(Vec::new()))?;
        set(&mut state, "error", Value::text(""))?;
        storage::write(config, &mut state)?;
    }
    println!("[vision_test] stopped");
    Ok(0)
}
