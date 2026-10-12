use openpilot_cluster::{
    route_replay::{
        preload::{self, Config, ParseRequest, Response, Worker},
        Error,
    },
    runtime_args::CornerSource,
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Query {
    requests: Vec<ParseRequest>,
    restart_path: PathBuf,
    blocked_path: PathBuf,
    launcher: PathBuf,
}
fn value(response: Response) -> serde_json::Value {
    let (frames, error) = match response.result {
        Ok(frames) => (Some(frames), None),
        Err(error) => (None, Some(error)),
    };
    serde_json::json!({"generation": response.generation, "index": response.index, "path": response.path, "frames": frames, "error": error})
}
fn response(worker: &mut Worker) -> Result<serde_json::Value, Error> {
    Ok(value(worker.receive(true)?.ok_or_else(|| {
        Error::Invalid("expected preload response".into())
    })?))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|arg| arg == "--route-preload-helper")
    {
        let source = match args.get(3).map(String::as_str) {
            Some("raw") => CornerSource::Raw,
            Some("stable") => CornerSource::Stable,
            _ => CornerSource::Live,
        };
        return Ok(preload::run_helper(
            Path::new(args.get(2).ok_or("socket path missing")?),
            source,
        )?);
    }
    let query: Query = serde_json::from_reader(std::io::stdin().lock())?;
    let mut worker = Worker::new(Config {
        program: std::env::current_exe()?,
        launcher: query.launcher,
        cwd: std::env::current_dir()?,
        corner_source: CornerSource::Live,
    })?;
    let mut results = Vec::new();
    for request in query.requests {
        worker.request(request)?;
        results.push(response(&mut worker)?);
    }
    worker.restart()?;
    worker.request(ParseRequest {
        generation: 5,
        index: 6,
        path: query.restart_path,
    })?;
    results.push(response(&mut worker)?);
    let idle_pid = worker.process_id().ok_or("idle helper missing")?;
    let start = Instant::now();
    worker.close()?;
    let idle_seconds = start.elapsed().as_secs_f64();
    worker.restart()?;
    let blocked_pid = worker.process_id().ok_or("blocked helper missing")?;
    worker.request(ParseRequest {
        generation: 6,
        index: 7,
        path: query.blocked_path,
    })?;
    std::thread::sleep(Duration::from_millis(150));
    let start = Instant::now();
    worker.close()?;
    let blocked_seconds = start.elapsed().as_secs_f64();
    worker.restart()?;
    let dead_pid = worker.process_id().ok_or("helper missing")?;
    let pid =
        rustix::process::Pid::from_raw(i32::try_from(dead_pid)?).ok_or("invalid helper PID")?;
    rustix::process::kill_process(pid, rustix::process::Signal::TERM)?;
    std::thread::sleep(Duration::from_millis(20));
    let empty_poll = worker.receive(false)?.is_none();
    let exit_detected = worker.receive(true).is_err();
    worker.close()?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({"responses": results,
            "idle_seconds": idle_seconds, "blocked_seconds": blocked_seconds,
            "idle_reaped": !Path::new(&format!("/proc/{idle_pid}")).exists(),
        "blocked_reaped": !Path::new(&format!("/proc/{blocked_pid}")).exists(),
        "empty_poll": empty_poll, "exit_detected": exit_detected,
        }))?
    );
    Ok(())
}
