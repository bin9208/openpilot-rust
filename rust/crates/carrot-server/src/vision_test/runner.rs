use super::{control, status, storage, Config};
use crate::{json_fields::set, Error, Value};
use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Children(Vec<(String, Child)>);
impl Children {
    fn start(&mut self, config: &Config, index: usize, state: &mut Value) -> Result<(), Error> {
        let (name, spec) = &config.children[index];
        let stderr =
            rustix::io::fcntl_dupfd_cloexec(std::io::stdout(), 3).map_err(std::io::Error::from)?;
        let child = Command::new(&spec.path)
            .args(&spec.args)
            .current_dir(&config.repository)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| crate::state::io_error(error, &spec.path))?;
        let pid = child.id();
        self.0.push((name.clone(), child));
        let mut children = state.get("children").clone();
        set(&mut children, name, Value::integer(pid))?;
        set(state, "children", children)?;
        storage::write(config, state)?;
        println!("[vision_test] {name} started pid={pid}");
        Ok(())
    }
    fn exited(&mut self) -> Result<Option<(String, i32)>, Error> {
        use std::os::unix::process::ExitStatusExt;
        for (name, child) in &mut self.0 {
            if let Some(status) = child.try_wait()? {
                return Ok(Some((
                    name.clone(),
                    status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(0)),
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
struct Snapshot<'a> {
    config: &'a Config,
    active: bool,
}
impl Drop for Snapshot<'_> {
    fn drop(&mut self) {
        if self.active {
            if let Err(error) = self.config.snapshot(false) {
                eprintln!("[vision_test] snapshot cleanup: {error}");
            }
        }
    }
}
pub(super) async fn run(config: &Config) -> Result<i32, Error> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut state = Value::object([
        ("status", Value::text("starting")),
        ("runner_pid", Value::integer(std::process::id())),
        ("started_at", Value::Float(storage::now())),
        ("children", Value::Object(Vec::new())),
        ("error", Value::text("")),
    ]);
    storage::write(config, &mut state)?;
    let mut children = Children(Vec::new());
    let mut snapshot = Snapshot {
        config,
        active: true,
    };
    let result: Result<(), Error> = async {
        if !config.boolean("IsOffroad")? { return Err(Error::Source("device is not offroad".into())); }
        config.snapshot(true)?;
        println!("[vision_test] IsTakingSnapshot enabled");
        tokio::time::sleep(Duration::from_secs(2)).await;
        children.start(config, 0, &mut state)?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        let mut streams = Vec::new();
        while tokio::time::Instant::now() < deadline {
            streams = crate::youtube_test::status::streams();
            if !streams.is_empty() { break; }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if streams.is_empty() { return Err(Error::Source("camerad did not publish VisionIPC streams".into())); }
        println!("[vision_test] VIPC streams ready: {}", streams.iter().map(i32::to_string).collect::<Vec<_>>().join(","));
        children.start(config, 1, &mut state)?;
        children.start(config, 2, &mut state)?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        let mut ready = false;
        while tokio::time::Instant::now() < deadline {
            ready = status::port(config);
            if ready { break; }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if !ready { return Err(Error::Source(format!("webrtcd did not open port {}", config.port))); }
        println!("[vision_test] webrtcd port ready: {}", config.port);
        set(&mut state, "status", Value::text("running"))?;
        storage::write(config, &mut state)?;
        println!("[vision_test] running timeout=600s");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        while tokio::time::Instant::now() < deadline {
            if !config.boolean("IsOffroad")? { println!("[vision_test] offroad ended; stopping"); break; }
            if let Some((name, code)) = children.exited()? { return Err(Error::Source(format!("{name} exited code={code}"))); }
            tokio::select! { _ = term.recv() => break, _ = interrupt.recv() => break, _ = tokio::time::sleep(Duration::from_millis(500)) => {} }
        }
        if tokio::time::Instant::now() >= deadline { println!("[vision_test] timeout reached; stopping"); }
        Ok(())
    }.await;
    if let Err(error) = &result {
        set(&mut state, "status", Value::text("error"))?;
        set(&mut state, "error", Value::text(&error.to_string()))?;
        storage::write(config, &mut state)?;
        println!("[vision_test] error: {error}");
    }
    control::children(config, &state).await?;
    children.reap()?;
    for (name, _) in children.0.iter().rev() {
        println!("[vision_test] {name} stopped");
    }
    config.snapshot(false)?;
    snapshot.active = false;
    set(&mut state, "children", Value::Object(Vec::new()))?;
    if !state.get("status").text_eq("error") {
        set(&mut state, "status", Value::text("stopped"))?;
    }
    storage::write(config, &mut state)?;
    println!("[vision_test] IsTakingSnapshot cleared");
    Ok(i32::from(result.is_err()))
}
