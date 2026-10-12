use super::{cleanup, config::Config, output, process, status, storage};
use crate::{youtube_live::profiles, Error, Value};
use openpilot_process_supervision::CapturedCommand;
use std::{fs, process::Stdio, time::Duration};

pub(super) async fn start(config: &Config, announce: bool) -> Result<i32, Error> {
    let status = status::get(config)?;
    if status.get("runner_alive").truth() {
        println!("[youtube-test] already running");
        return output::print(config, None);
    }
    let refusal = if !status::boolean(config, "IsOffroad") {
        Some("device is not offroad")
    } else if status::boolean(config, "IsTakingSnapshot") {
        Some("another camera test is active")
    } else if status::param_integer(config, "CarrotYouTubeTimestamp") > 0 {
        Some("disable CarrotYouTubeTimestamp first")
    } else if storage::configured(storage::json(&config.paths.secret).get("stream_key")) {
        None
    } else {
        Some("YouTube stream key is not configured")
    };
    if let Some(message) = refusal {
        eprintln!("[youtube-test] refused: {message}");
        return Ok(1);
    }
    let mut conflicts = vec![
        ("camerad".to_owned(), config.camera.pattern.clone()),
        (
            "carrot_vision_encoderd".into(),
            format!("{}\0--carrot-vision-road", config.encoder.display()),
        ),
    ];
    conflicts.extend(profiles::PROFILES.into_iter().map(|profile| {
        (
            profile.process.into(),
            config.encoder(i32::from(profile.quality)).pattern,
        )
    }));
    for (name, pattern) in conflicts {
        let pids = process::matching(&pattern);
        if !pids.is_empty() {
            eprintln!(
                "[youtube-test] refused: {name} already running pid={}",
                pids.iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            return Ok(1);
        }
    }
    let quality = status::param_integer(config, "CarrotYouTubeQuality");
    let quality = if (0..=3).contains(&quality) {
        quality
    } else {
        0
    };
    fs::write(&config.paths.log, "")?;
    let log = fs::OpenOptions::new()
        .append(true)
        .open(&config.paths.log)?;
    let mut argv = vec![config.runner.path.as_os_str().to_owned()];
    argv.extend(config.runner.args.iter().cloned());
    argv.extend([
        "_run".into(),
        "--quality".into(),
        quality.to_string().into(),
    ]);
    let mut child = CapturedCommand {
        launcher: config.launcher.clone(),
        cwd: config.repository.clone(),
        argv,
    }
    .spawn_session_redirected(Stdio::from(log.try_clone()?), Stdio::from(log))
    .map_err(|error| Error::Source(error.to_string()))?;
    println!("[youtube-test] starting runner pid={}", child.process.id());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let status = status::get(config)?;
        let state = storage::text(status.get("status"));
        if state == "running" {
            if announce {
                println!("[youtube-test] ready; run 'carrot youtube-test status' after the upload metrics settle");
                return output::print(config, Some(status));
            }
            println!("[youtube-test] pipeline ready; collecting verification metrics");
            return Ok(0);
        }
        if state == "error" {
            eprintln!(
                "[youtube-test] failed: {}",
                if status.get("error").truth() {
                    storage::text(status.get("error"))
                } else {
                    "unknown error".into()
                }
            );
            return Ok(1);
        }
        if child.process.try_wait()?.is_some() {
            eprintln!("[youtube-test] runner exited during startup");
            return Ok(1);
        }
    }
    eprintln!("[youtube-test] startup is still in progress; run 'carrot youtube-test status'");
    Ok(1)
}
pub(super) async fn stop(config: &Config) -> Result<i32, Error> {
    let mut state = storage::json(&config.paths.state);
    let pid = i32::try_from(storage::integer(state.get("runner_pid"))?).unwrap_or(0);
    if process::alive(pid, &config.runner.pattern) {
        println!("[youtube-test] stopping runner pid={pid}");
        process::terminate(pid, &config.runner.pattern, Duration::from_secs(15)).await?;
    } else if state.truth() {
        println!("[youtube-test] runner is not active; cleaning stale state");
        cleanup::children(config, &state).await?;
        cleanup::restore(config, &state)?;
        cleanup::snapshot(config, false)?;
    } else {
        println!("[youtube-test] runner is not active");
    }
    state = storage::json(&config.paths.state);
    if state.truth() {
        storage::set(&mut state, "status", Value::text("stopped"));
        storage::set(&mut state, "children", Value::Object(Vec::new()));
        storage::set(&mut state, "error", Value::text(""));
        storage::write(config, &mut state)?;
    }
    println!("[youtube-test] stopped");
    Ok(0)
}
