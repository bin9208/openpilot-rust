//! Actual source/native loggerd comparator, source-authored full Event inputs.
//! Invoke only from the dedicated CLI. The parent alone owns global subreaping
//! and normal diagnostic receivers; workers own their per-case environment.
use crate::{
    argparse_scan::{scan_os_required_first, ArgumentError, OptionSpec},
    seal::Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
pub mod controllers;
mod edges;
mod fixtures;
mod hevc;
mod input_completion;
pub mod io;
mod owned;
mod peer;
mod process;
mod providers;
mod receiver;
mod scenarios;
mod validation;
const WORKER: &str = "--internal-loggerd-worker";
const IDLE: &str = "--internal-loggerd-idle-fixture";
const SCENARIOS: [&str; 7] = [
    "ordinary",
    "burst",
    "fairness",
    "video",
    "audio",
    "audio-queue",
    "queue-restart",
];
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct Request {
    kind: String,
    name: String,
    binary: PathBuf,
    output: PathBuf,
    fixtures: PathBuf,
    runner: Vec<String>,
}
fn worker(request: &Request) -> Result<Value> {
    if [
        "diagnostic-throughput",
        "diagnostic-suppression",
        "pipeline",
    ]
    .contains(&request.kind.as_str())
    {
        return controllers::worker(request);
    }
    match request.kind.as_str() {
        "loggerd-io" => return io::worker(request),
        "fixture" => {
            let packets = fixtures::capture(&request.binary, &request.output, &request.name)?;
            return Ok(json!({"packets":packets.len()}));
        }
        "input-completion" => return input_completion::check(&request.output),
        _ => {}
    }
    let mut settings = match request.kind.as_str() {
        "scenario" => scenarios::settings(&request.name),
        "edge" => edges::settings(&request.name),
        "hevc" => peer::Settings::new(&["roadEncodeData"]),
        "fallback" => edges::fallback_settings(),
        _ => return Err("unknown internal logger worker kind".into()),
    };
    if request.name == "malformed" {
        rustix::process::setrlimit(
            rustix::process::Resource::Core,
            rustix::process::Rlimit {
                current: Some(0),
                maximum: Some(0),
            },
        )?;
    }
    settings.runner.clone_from(&request.runner);
    let mut peer = peer::Peer::start(&request.binary, &request.output, &settings)?;
    if request.kind == "hevc" {
        let paths: Vec<String> =
            serde_json::from_slice(&fs::read(request.fixtures.join("records.json"))?)?;
        for path in paths {
            require(
                !path.contains('/') && !path.contains(".."),
                "HEVC fixture path escaped",
            )?;
            peer.send("roadEncodeData", &fs::read(request.fixtures.join(path))?)?;
        }
        let logs = peer.stop(rustix::process::Signal::TERM)?;
        require(
            fs::read(peer.segment(0).join("fcamera.hevc"))?
                == fs::read(request.fixtures.join("native.hevc"))?,
            "HEVC raw writer changed actual bitstream bytes",
        )?;
        return Ok(json!({"logs":logs}));
    }
    let fixtures = fixtures::Fixtures::load(&request.fixtures)?;
    match request.kind.as_str() {
        "scenario" => {
            scenarios::drive(&mut peer, &request.name, &fixtures)?;
            let logs = peer.stop(rustix::process::Signal::TERM)?;
            scenarios::assert_logs(&request.name, &logs)?;
            Ok(json!({"logs":logs}))
        }
        "edge" => edges::run(&mut peer, &request.name, &fixtures),
        "fallback" => {
            let stuck = request.name == "stuck-encoder";
            let mut sequence = 0;
            let rotation;
            loop {
                sequence += 1;
                edges::fallback_tick(
                    &mut peer,
                    stuck,
                    sequence,
                    fixtures.packet("roadEncodeData", 0)?,
                )?;
                let elapsed = peer.started.elapsed().as_secs_f64();
                if peer.segment(1).is_dir() {
                    let minimum = if stuck { 72.0 } else { 60.0 };
                    require(
                        (minimum..=minimum + 2.0).contains(&elapsed),
                        "logger fallback rotation outside unchanged source window",
                    )?;
                    rotation = elapsed;
                    break;
                }
                require(
                    elapsed < 75.0,
                    "logger fallback did not rotate within source bound",
                )?;
                std::thread::sleep(Duration::from_millis(100));
            }
            save(
                &request.output.join("fallback-rotated.json"),
                &json!({"rotation_seconds":rotation}),
            )?;
            let stop = request.output.join("fallback-stop");
            let release_deadline = std::time::Instant::now() + Duration::from_secs(20);
            while !stop.exists() {
                require(
                    std::time::Instant::now() < release_deadline,
                    "fallback cohort release exceeded bound",
                )?;
                std::thread::sleep(Duration::from_millis(5));
            }
            let logs = peer.stop(rustix::process::Signal::TERM)?;
            require(
                logs.len() == 2
                    && logs[0]["rlog"]
                        .as_array()
                        .and_then(|v| v.last())
                        .is_some_and(|v| v["sentinel"]["type"] == "endOfSegment")
                    && logs[1]["rlog"][1]["sentinel"]["type"] == "startOfSegment",
                "fallback segment/sentinel sequence differs",
            )?;
            Ok(json!({"rotation_seconds":rotation,"segments":2}))
        }
        _ => Err("unknown internal logger worker kind".into()),
    }
}
fn request(
    kind: &str,
    name: &str,
    binary: &Path,
    output: &Path,
    fixtures: &Path,
    runner: &[String],
) -> Request {
    Request {
        kind: kind.into(),
        name: name.into(),
        binary: binary.into(),
        output: output.into(),
        fixtures: fixtures.into(),
        runner: runner.into(),
    }
}
struct Pair<'a> {
    root: &'a Path,
    source: &'a Path,
    native: &'a Path,
}
impl Pair<'_> {
    fn run(
        &self,
        owner: &mut owned::Owner,
        output: &Path,
        fixtures: &Path,
        kind: &str,
        name: &str,
        runner: &[String],
    ) -> Result<Value> {
        let Self {
            root,
            source,
            native,
        } = self;
        fs::create_dir_all(output)?;
        let a = output.join("original");
        let b = output.join("rust");
        owner.run(
            &[request(kind, name, source, &a, fixtures, &[])],
            Duration::from_secs(300),
        )?;
        owner.run(
            &[request(kind, name, native, &b, fixtures, runner)],
            Duration::from_secs(300),
        )?;
        let source = peer::Evidence::load(&a, true)?;
        let native = peer::Evidence::load(&b, true)?;
        let report = validation::compare(
            (&source, &source.records()?),
            (&native, &native.records()?),
            &owner.cancelled,
        )?;
        let diagnostics = validation::diagnostics(root, &source, &native)?;
        Ok(json!({"comparison":report,"diagnostics":diagnostics}))
    }
}
/// Entry point for a dedicated process, never a general library supervisor.
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 2 && args[0] == WORKER {
        let request: Request = serde_json::from_slice(&fs::read(&args[1])?)?;
        let result = worker(&request)?;
        return save(&request.output.join("worker-result.json"), &result);
    }
    if args.len() == 2 && args[0] == IDLE {
        return input_completion::fixture(Path::new(&args[1]));
    }
    let parsed = scan_os_required_first(
        &args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--original", true),
            OptionSpec::value("--producer", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--fixtures", false),
            OptionSpec::append("--scenario"),
            OptionSpec::flag("--edges", None),
            OptionSpec::flag("--fallbacks", None),
            OptionSpec::flag("--hevc", None),
            OptionSpec::value("--runner", false),
            OptionSpec::value("--root", false),
        ],
    )?;
    if parsed.help {
        println!("usage: check-loggerd-native --binary BINARY --original ORIGINAL --producer PRODUCER --output OUTPUT [--fixtures FIXTURES] [--scenario SCENARIO] [--edges] [--hevc] [--fallbacks] [--runner RUNNER] [--root ROOT]");
        return Ok(());
    }
    let binary = PathBuf::from(&parsed.values["--binary"]).canonicalize()?;
    let original = PathBuf::from(&parsed.values["--original"]).canonicalize()?;
    let producer = PathBuf::from(&parsed.values["--producer"]).canonicalize()?;
    let output = std::path::absolute(Path::new(&parsed.values["--output"]))?;
    let root = parsed
        .values
        .get("--root")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        )
        .canonicalize()?;
    let runner = parsed
        .values
        .get("--runner")
        .map(|v| {
            PathBuf::from(v).canonicalize().and_then(|v| {
                v.into_os_string()
                    .into_string()
                    .map_err(|_| std::io::Error::other("runner is not UTF-8"))
            })
        })
        .transpose()?
        .into_iter()
        .collect::<Vec<_>>();
    let scenarios = parsed
        .append_values
        .get("--scenario")
        .map(|rows| {
            rows.iter()
                .map(|v| {
                    v.to_str().map(str::to_owned).ok_or_else(|| {
                        Box::new(ArgumentError("scenario must be UTF-8".into()))
                            as Box<dyn std::error::Error>
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_else(|| SCENARIOS.map(str::to_owned).to_vec());
    for name in &scenarios {
        if !SCENARIOS.contains(&name.as_str()) {
            return Err(Box::new(ArgumentError(format!(
                "invalid choice for --scenario: {name}"
            ))));
        }
    }
    let edges = parsed.values.contains_key("--edges");
    let fallbacks = parsed.values.contains_key("--fallbacks");
    require(
        runner.is_empty() || !(edges || fallbacks),
        "edge/fallback runner scenarios must be invoked separately",
    )?;
    fs::create_dir_all(output.parent().ok_or("logger output parent absent")?)?;
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(&output)?;
    let mut provider_paths = vec![original.as_path(), binary.as_path(), producer.as_path()];
    let runner_path = runner.first().map(PathBuf::from);
    if let Some(path) = &runner_path {
        provider_paths.push(path.as_path());
    }
    let mut providers = providers::Providers::new(&provider_paths, &output)?;
    let mut owner = owned::Owner::new(&output.join("control"))?;
    owner.run(
        &[request(
            "input-completion",
            "input-completion",
            &binary,
            &output.join("input-completion"),
            Path::new(""),
            &[],
        )],
        Duration::from_secs(30),
    )?;
    let fixtures = if let Some(path) = parsed.values.get("--fixtures") {
        PathBuf::from(path).canonicalize()?
    } else {
        let fixtures = output.join("fixtures");
        fs::create_dir(&fixtures)?;
        for kind in ["road", "qroad"] {
            owner.run(
                &[request(
                    "fixture",
                    kind,
                    &producer,
                    &fixtures.join(kind),
                    Path::new(""),
                    &[],
                )],
                Duration::from_secs(300),
            )?;
        }
        fixtures
    };
    let _ = fixtures::Fixtures::load(&fixtures)?;
    let pair = Pair {
        root: &root,
        source: &original,
        native: &binary,
    };
    let mut results = json!({});
    for name in scenarios {
        results[&name] = pair.run(
            &mut owner,
            &output.join(&name),
            &fixtures,
            "scenario",
            &name,
            &runner,
        )?;
        println!("{name} passed");
        save(&output.join("results.json"), &results)?;
    }
    let mut diagnostic_pairs = Vec::new();
    if edges {
        for name in ["disabled", "selection"] {
            results["edges"][name] = pair.run(
                &mut owner,
                &output.join("edges").join(name),
                &fixtures,
                "edge",
                name,
                &[],
            )?;
        }
        for (implementation, binary) in [("original", &original), ("rust", &binary)] {
            for name in [
                "missing-audio",
                "malformed",
                "signal-2",
                "signal-15",
                "signal-30",
            ] {
                let target = output
                    .join("edges")
                    .join(format!("{name}-{implementation}"));
                fs::create_dir_all(target.parent().ok_or("edge parent absent")?)?;
                results["edges"][name][implementation] = owner
                    .run(
                        &[request("edge", name, binary, &target, &fixtures, &[])],
                        Duration::from_secs(90),
                    )?
                    .remove(0);
            }
        }
        for name in [
            "missing-audio",
            "malformed",
            "signal-2",
            "signal-15",
            "signal-30",
        ] {
            diagnostic_pairs.push((
                output.join("edges").join(format!("{name}-original")),
                output.join("edges").join(format!("{name}-rust")),
            ));
        }
        let original = peer::Evidence::load(&output.join("edges/missing-audio-original"), false)?;
        let native = peer::Evidence::load(&output.join("edges/missing-audio-rust"), false)?;
        require(
            validation::normalize(&original.records()?[0]["rlog"])?
                == validation::normalize(&native.records()?[0]["rlog"])?,
            "missing-audio full rlog records differ",
        )?;
        println!("edges passed");
    }
    if parsed.values.contains_key("--hevc") {
        let path = output.join("hevc-fixture");
        let sample = hevc::fixture(&path, &owner.cancelled)?;
        require(
            fs::read(path.join("native.hevc"))? == sample.raw,
            "HEVC fixture changed",
        )?;
        let mut paths = Vec::new();
        for (i, packet) in sample.records.iter().enumerate() {
            let name = format!("{i:04}.capnp");
            fs::write(path.join(&name), packet)?;
            paths.push(name);
        }
        save(&path.join("records.json"), &json!(paths))?;
        results["hevc"] = pair.run(
            &mut owner,
            &output.join("hevc"),
            &path,
            "hevc",
            "hevc",
            &runner,
        )?;
        println!("hevc passed");
    }
    if fallbacks {
        let path = output.join("fallbacks");
        fs::create_dir(&path)?;
        let mut requests = Vec::new();
        for name in ["no-camera", "stuck-encoder"] {
            for (implementation, binary) in [("original", &original), ("rust", &binary)] {
                requests.push(request(
                    "fallback",
                    name,
                    binary,
                    &path.join(format!("{name}-{implementation}")),
                    &fixtures,
                    &[],
                ));
            }
        }
        let rows = owner.run(&requests, Duration::from_secs(100))?;
        results["fallbacks"] = json!(rows);
        for name in ["no-camera", "stuck-encoder"] {
            diagnostic_pairs.push((
                path.join(format!("{name}-original")),
                path.join(format!("{name}-rust")),
            ));
        }
        println!("fallbacks passed");
    }
    for (index, (original, native)) in diagnostic_pairs.iter().enumerate() {
        results["edge_and_fallback_diagnostics"][index.to_string()] = validation::diagnostics(
            &root,
            &peer::Evidence::load(original, false)?,
            &peer::Evidence::load(native, false)?,
        )?;
    }
    owner.finish()?;
    providers.verify()?;
    results["result"] = json!("pass");
    save(&output.join("results.json"), &results)
}

#[cfg(test)]
mod original_replay;

#[derive(Debug)]
pub struct Interrupted(pub i32);
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "logger QA interrupted by signal {}", self.0)
    }
}
impl std::error::Error for Interrupted {}

/// Standalone replacement for check_loggerd_peer.py; runs no logger daemon.
pub fn peer_cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 2 && args[0] == IDLE {
        return input_completion::fixture(Path::new(&args[1]));
    }
    if args.len() == 2 && args[0] == WORKER {
        let request: Request = serde_json::from_slice(&fs::read(&args[1])?)?;
        require(
            request.kind == "input-completion",
            "unknown peer worker kind",
        )?;
        let result = input_completion::check(&request.output)?;
        return save(&request.output.join("worker-result.json"), &result);
    }
    let parsed = scan_os_required_first(&args, &[OptionSpec::value("--output", true)])?;
    if parsed.help {
        println!("usage: check-loggerd-peer --output OUTPUT");
        return Ok(());
    }
    let output = std::path::absolute(Path::new(&parsed.values["--output"]))?;
    fs::create_dir_all(output.parent().ok_or("peer output parent absent")?)?;
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(&output)?;
    let executable = std::env::current_exe()?;
    let mut providers = providers::Providers::new(&[&executable], &output)?;
    let mut owner = owned::Owner::new_for(&output.join("control"), "check-loggerd-peer")?;
    let mut rows = owner.run(
        &[request(
            "input-completion",
            "input-completion",
            &executable,
            &output.join("busy-recipient"),
            Path::new(""),
            &[],
        )],
        Duration::from_secs(30),
    )?;
    let result = rows.pop().ok_or("peer result absent")?;
    owner.finish()?;
    providers.verify()?;
    owner.check_cancelled()?;
    let receipt = output.join("result.json");
    save(&receipt, &result)?;
    if let Err(error) = owner.check_cancelled() {
        fs::remove_file(receipt)?;
        return Err(error);
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

/// Test/QA adapter to the exact observer used by the live logger peer.
/// The caller owns and reaps this direct child; this function never signals it.
pub fn observe_logger_idle(child: &mut std::process::Child, timeout: Duration) -> Result<()> {
    let pid = rustix::process::Pid::from_child(child);
    peer::await_idle_pid(
        pid,
        || {
            require(
                child.try_wait()?.is_none(),
                "logger exited before input completion",
            )
        },
        timeout,
    )
}
