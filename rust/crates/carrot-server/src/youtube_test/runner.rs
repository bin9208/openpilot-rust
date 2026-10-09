use super::{
    cleanup,
    config::{CommandSpec, Config},
    poll, status, storage,
};
use crate::{youtube_live::profiles, Error, Value};
use std::{
    ops::{Deref, DerefMut},
    os::unix::process::ExitStatusExt,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct State<'a> {
    config: &'a Config,
    value: Value,
    active: bool,
}
impl Deref for State<'_> {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.value
    }
}
impl DerefMut for State<'_> {
    fn deref_mut(&mut self) -> &mut Value {
        &mut self.value
    }
}
impl Drop for State<'_> {
    fn drop(&mut self) {
        if self.active {
            if let Err(error) = cleanup::restore(self.config, &self.value) {
                eprintln!("[youtube-test] live cleanup: {error}");
            }
            if let Err(error) = cleanup::snapshot(self.config, false) {
                eprintln!("[youtube-test] snapshot cleanup: {error}");
            }
        }
    }
}

struct Children(Vec<(String, Child)>);
impl Children {
    fn start(
        &mut self,
        config: &Config,
        spec: (String, CommandSpec),
        state: &mut Value,
    ) -> Result<(), Error> {
        let (name, command) = spec;
        let child = Command::new(&command.path)
            .args(command.args)
            .current_dir(&config.repository)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| crate::state::io_error(error, &command.path))?;
        let pid = child.id();
        self.0.push((name.clone(), child));
        let mut children = state.get("children").clone();
        storage::set(&mut children, &name, Value::integer(pid));
        storage::set(state, "children", children);
        storage::write(config, state)?;
        println!("[youtube-test] {name} started pid={pid}");
        Ok(())
    }
    fn exited(&mut self) -> Result<Option<(String, String)>, Error> {
        for (name, child) in &mut self.0 {
            if let Some(status) = child.try_wait()? {
                return Ok(Some((
                    name.clone(),
                    status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(0))
                        .to_string(),
                )));
            }
        }
        Ok(None)
    }
    fn reap(&mut self) -> Result<(), Error> {
        for (_, child) in &mut self.0 {
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
        }
        Ok(())
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        for (_, child) in self.0.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                let _killed = child.kill();
                let _reaped = child.wait();
            }
        }
    }
}
pub async fn run(config: &Config, quality: i32) -> Result<i32, Error> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let previous = status::param_integer(config, "CarrotYouTubeLive");
    let profile = profiles::selected(quality);
    let mut state = State {
        config,
        active: true,
        value: Value::object([
            ("status", Value::text("starting")),
            ("runner_pid", Value::integer(std::process::id())),
            ("started_mono", Value::Float(storage::monotonic())),
            ("quality", Value::integer(quality)),
            ("quality_label", Value::text(profile.label)),
            ("previous_live", Value::integer(previous)),
            ("forced_live", Value::Bool(previous <= 0)),
            ("children", Value::Object(Vec::new())),
            ("source_sample", Value::Object(Vec::new())),
            ("error", Value::text("")),
        ]),
    };
    storage::write(config, &mut state)?;
    let mut children = Children(Vec::new());
    let result: Result<(), Error> = async {
        if !status::boolean(config, "IsOffroad") { return Err(Error::Source("device is not offroad".into())); }
        if status::param_integer(config, "CarrotYouTubeTimestamp") > 0 { return Err(Error::Source("CarrotYouTubeTimestamp must be disabled".into())); }
        cleanup::snapshot(config, true)?;
        println!("[youtube-test] IsTakingSnapshot enabled");
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut socket = poll::subscribe()?;
        let [camera, encoder] = config.child_specs(quality);
        children.start(config, camera, &mut state)?;
        let streams = poll::vipc(Duration::from_secs(8)).await;
        if streams.is_empty() { return Err(Error::Source("camerad did not publish VisionIPC streams".into())); }
        println!("[youtube-test] VIPC streams ready: {}", streams.iter().map(i32::to_string).collect::<Vec<_>>().join(","));
        children.start(config, encoder, &mut state)?;
        let sample = poll::keyframe(&mut socket, Duration::from_secs(12)).await?;
        if !sample.truth() { return Err(Error::Source(format!("{} did not publish an H.264 keyframe", profile.process))); }
        println!("[youtube-test] H.264 keyframe ready: {}x{} header={} frame={}", sample.get("width").encode()?, sample.get("height").encode()?, sample.get("header_bytes").encode()?, sample.get("frame_bytes").encode()?);
        storage::set(&mut state, "source_sample", sample);
        if previous <= 0 { config.params.put("CarrotYouTubeLive", b"1")?; println!("[youtube-test] CarrotYouTubeLive enabled for this test"); }
        storage::set(&mut state, "status", Value::text("running"));
        storage::write(config, &mut state)?;
        println!("[youtube-test] running timeout=600s quality={}", profile.label);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        let mut last = String::new();
        while tokio::time::Instant::now() < deadline {
            if !status::boolean(config, "IsOffroad") { println!("[youtube-test] offroad ended; stopping"); break; }
            if let Some((name, code)) = children.exited()? { return Err(Error::Source(format!("{name} exited code={code}"))); }
            let youtube = status::youtube(config);
            let current = storage::text(youtube.get("state"));
            storage::set(&mut state, "youtube", youtube);
            storage::write(config, &mut state)?;
            if !current.is_empty() && current != last { println!("[youtube-test] YouTube state: {current}"); last = current; }
            tokio::select! { _ = term.recv() => break, _ = interrupt.recv() => break, _ = tokio::time::sleep(Duration::from_millis(500)) => {} }
        }
        if tokio::time::Instant::now() >= deadline { println!("[youtube-test] timeout reached; stopping"); }
        Ok(())
    }.await;
    if let Err(error) = &result {
        storage::set(&mut state, "status", Value::text("error"));
        storage::set(&mut state, "error", Value::text(&error.to_string()));
        storage::write(config, &mut state)?;
        println!("[youtube-test] error: {error}");
    }
    cleanup::children(config, &state).await?;
    children.reap()?;
    for (name, _) in children.0.iter().rev() {
        println!("[youtube-test] {name} stopped");
    }
    cleanup::restore(config, &state)?;
    if state.get("forced_live").truth() {
        println!("[youtube-test] CarrotYouTubeLive restored");
    }
    cleanup::snapshot(config, false)?;
    state.active = false;
    storage::set(&mut state, "children", Value::Object(Vec::new()));
    if storage::text(state.get("status")) != "error" {
        storage::set(&mut state, "status", Value::text("stopped"));
    }
    storage::write(config, &mut state)?;
    println!("[youtube-test] IsTakingSnapshot cleared");
    Ok(if result.is_ok() { 0 } else { 1 })
}
