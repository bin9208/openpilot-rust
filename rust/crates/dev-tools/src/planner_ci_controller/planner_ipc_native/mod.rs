//! 실제 Planner IPC40×4 orchestration. 입력은 독립 raw corpus, transport는 원래 C++ msgq다.
//! 부모는 이 module/CLI/feature/plan을 등록해야 한다. corpus가 없으면 출력 전 거부한다.
use super::{scheduler, scheduler_client};
use crate::{
    seal::{digest, Result},
    services_header,
};
use rustix::{
    process::{kill_process, Pid, Signal},
    time::{clock_gettime, ClockId},
};
use scheduler_client::SchedulerClient;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub mod corpus;
pub mod decode;
pub mod frame_loop;
#[cfg(test)]
mod observed_tests;
pub mod outputs;
mod peer;
use corpus::{Capture, Corpus, RawFile};
use decode::Record;
use frame_loop::{drive40, FrameEvents};
use outputs::OriginalOutputs;
use peer::Peer;

const OUTPUTS: [&str; 3] = ["longitudinalPlan", "lateralPlan", "driverAssistance"];
const PEER_FILES: [&str; 6] = [
    "rust/tools/planner_ipc_peer.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
];

pub struct Options {
    pub root: PathBuf,
    pub python: PathBuf,
    pub binary: PathBuf,
    pub artifact: PathBuf,
    pub binding: PathBuf,
    pub source_native: PathBuf,
    pub original_ipc_binding: PathBuf,
    pub scheduler: PathBuf,
    pub compiler: PathBuf,
    /// 보존 private/functional ZIP과 각 안전 추출 디렉터리가 있는 audit bundle root다.
    pub corpus: PathBuf,
    pub corpus_audit: PathBuf,
    pub corpus_audit_sha256: String,
    pub output: PathBuf,
    pub evidence: PathBuf,
    pub timeout: Duration,
    pub growth_bytes: u64,
    pub peer_build_growth_bytes: u64,
}

/// 기존 isolated build 산출물을 native 파일 조회로 찾는다. Python import/query를 추가하지 않는다.
pub fn original_ipc_binding(directory: &Path) -> Result<PathBuf> {
    if directory.canonicalize()? != directory || !fs::symlink_metadata(directory)?.is_dir() {
        return Err("canonical original IPC module directory required".into());
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|name| name.starts_with("ipc_pyx.") && name.ends_with(".so"))
        {
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("original IPC binding must be a regular non-symlink".into());
            }
            found.push(entry.path());
        }
    }
    if found.len() != 1 {
        return Err("exactly one original ipc_pyx module is required".into());
    }
    Ok(found.remove(0))
}

fn now() -> f64 {
    let time = clock_gettime(ClockId::Monotonic);
    (time.tv_sec as f64 * 1_000_000_000.0 + time.tv_nsec as f64) / 1_000_000_000.0
}
fn pause(seconds: f64, deadline: Instant) -> Result<()> {
    if !seconds.is_finite() {
        return Err("nonfinite IPC wait".into());
    }
    if Instant::now() >= deadline {
        return Err("Planner IPC operation deadline".into());
    }
    let duration = Duration::from_secs_f64(seconds.max(0.0));
    if duration > deadline.saturating_duration_since(Instant::now()) {
        return Err("Planner IPC wait exceeds operation deadline".into());
    }
    std::thread::sleep(duration);
    Ok(())
}
fn limit(deadline: Instant, seconds: u64) -> Instant {
    deadline.min(Instant::now() + Duration::from_secs(seconds))
}
fn text(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .ok_or("Planner IPC paths must be UTF-8")?
        .into())
}
fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

struct ChildOwner {
    child: Child,
    reaped: bool,
}
impl ChildOwner {
    fn running(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            self.reaped = true;
            return Err(format!("daemon exited while processing: {status}").into());
        }
        Ok(())
    }
    fn finish_daemon<T>(&mut self, mut work: Result<T>) -> Result<(T, i32)> {
        let mut killed = work.is_err();
        if self.child.try_wait()?.is_none() {
            if work.is_err() {
                self.child.kill()?;
            } else {
                let pid =
                    Pid::from_raw(self.child.id().try_into()?).ok_or("invalid planner PID")?;
                kill_process(pid, Signal::TERM)?;
            }
        }
        let mut deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.reaped = true;
                let code = status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1));
                return match work {
                    Err(error) => Err(error),
                    Ok(value) if code == -15 => Ok((value, code)),
                    Ok(_) => Err(format!("Planner IPC daemon close status differs: {code}").into()),
                };
            }
            if Instant::now() >= deadline {
                if !killed {
                    // 원본의 wait(3) 실패 뒤 finally의 KILL/wait(3)를 별도로 보존한다.
                    work = Err("Planner daemon TERM wait exceeded 3 seconds".into());
                    self.child.kill()?;
                    killed = true;
                    deadline = Instant::now() + Duration::from_secs(3);
                    continue;
                }
                let _ = self.child.kill();
                return Err("Planner daemon KILL wait exceeded 3 seconds".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if !self.reaped {
            // 직접 child만 회수한다. compiler 자손 group은 상위 IPC caller 소유다.
            let deadline = Instant::now() + Duration::from_secs(3);
            let _ = self.child.kill();
            loop {
                match self.child.try_wait() {
                    Ok(Some(_)) => {
                        self.reaped = true;
                        break;
                    }
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    _ => break,
                }
            }
        }
    }
}

fn stage(corpus: &Corpus, raw: &RawFile, case: &Path) -> Result<PathBuf> {
    let bytes = corpus.read(raw)?;
    let path = case.join("inputs").join(format!("{}.bin", raw.sha256));
    if path.exists() {
        if fs::read(&path)? != bytes {
            return Err("staged original packet changed".into());
        }
    } else {
        fs::write(&path, bytes)?;
    }
    Ok(path)
}
struct Events<'a> {
    peer: &'a mut Peer,
    daemon: &'a mut ChildOwner,
    corpus: &'a Corpus,
    case: &'a Path,
    records: Vec<Record>,
    deadline: Instant,
}
impl Events<'_> {
    fn count(&self, service: &str) -> usize {
        self.records
            .iter()
            .filter(|row| row.service == service)
            .count()
    }
    fn drain(&mut self, deadline: Instant) -> Result<()> {
        for (service, path) in self.peer.drain(self.case, deadline)? {
            if !OUTPUTS.contains(&service.as_str()) {
                return Err("unexpected Planner output service".into());
            }
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 16 * 1024 * 1024
            {
                return Err("invalid C++ raw publication".into());
            }
            self.records
                .push(decode::packet(&fs::read(&path)?, &service, text(&path)?)?);
        }
        Ok(())
    }
    fn wait_for(&mut self, service: &str, before: usize) -> Result<()> {
        let deadline = limit(self.deadline, 2);
        while Instant::now() < deadline {
            self.drain(deadline)?;
            if self.count(service) > before {
                return Ok(());
            }
            self.daemon.running()?;
            pause(0.001, deadline)?;
        }
        Err(format!("missing {service} publication").into())
    }
}
impl FrameEvents for Events<'_> {
    fn sleep_until(&mut self, target: f64) -> Result<()> {
        pause(target - now(), self.deadline)
    }
    fn model(&mut self, index: usize) -> Result<(f64, f64)> {
        let frame = &self.corpus.manifest.frames[index];
        let packets = frame
            .packets
            .iter()
            .map(|packet| {
                Ok((
                    packet.service.clone(),
                    stage(self.corpus, &packet.raw, self.case)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let before = self.count("driverAssistance");
        let sent = now();
        for (service, path) in packets {
            self.peer
                .publish(&service, &path, limit(self.deadline, 2))?;
        }
        self.wait_for("driverAssistance", before)?;
        Ok((sent, now()))
    }
    fn live(&mut self, index: usize) -> Result<f64> {
        let path = stage(
            self.corpus,
            &self.corpus.manifest.frames[index].live,
            self.case,
        )?;
        let before = self.count("longitudinalPlan");
        self.peer
            .publish("liveTracks", &path, limit(self.deadline, 2))?;
        if index >= 1 && !(12..16).contains(&index) {
            self.wait_for("longitudinalPlan", before)?;
        } else {
            pause(0.005, self.deadline)?;
            self.drain(limit(self.deadline, 2))?;
        }
        Ok(now())
    }
}

fn build_peer(options: &Options, deadline: Instant) -> Result<(PathBuf, Value)> {
    openpilot_disk_space::inspect(&options.output, options.peer_build_growth_bytes)?.require()?;
    let include = options.output.join("peer-include");
    fs::create_dir(&include)?;
    let header = include.join("services.h");
    fs::write(&header, services_header::header(true))?;
    let sources = PEER_FILES
        .iter()
        .map(|name| Ok(((*name).to_owned(), digest(&options.root.join(name))?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let headers = ["ipc.h", "event.h", "impl_msgq.h", "impl_fake.h", "msgq.h"]
        .iter()
        .map(|name| {
            let path = options.root.join("msgq_repo/msgq").join(name);
            Ok(((*name).to_owned(), digest(&path)?))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let binary = options.output.join("planner-ipc-cpp-peer");
    let compiler_sha256 = digest(&options.compiler)?;
    let mut argv = vec![
        options.compiler.as_os_str().to_owned(),
        "-std=c++17".into(),
        "-UNDEBUG".into(),
        "-pthread".into(),
        format!("-I{}", options.root.join("msgq_repo").display()).into(),
        format!("-I{}", include.display()).into(),
    ];
    argv.extend(
        PEER_FILES
            .iter()
            .map(|path| options.root.join(path).into_os_string()),
    );
    argv.extend(["-o".into(), binary.clone().into_os_string()]);
    // compiler도 상위 IPC QA group을 상속한다. nested detached group을 만들지 않는다.
    let child = Command::new(&options.compiler)
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::from(fs::File::create(
            options.output.join("peer-build.stdout"),
        )?))
        .stderr(Stdio::from(fs::File::create(
            options.output.join("peer-build.stderr"),
        )?))
        .spawn()?;
    let mut compiler_child = ChildOwner {
        child,
        reaped: false,
    };
    let build_deadline = limit(deadline, 180);
    loop {
        if let Some(status) = compiler_child.child.try_wait()? {
            compiler_child.reaped = true;
            if !status.success() {
                return Err(format!("planner C++ peer compiler exited {status}").into());
            }
            break;
        }
        if Instant::now() >= build_deadline {
            return Err("planner C++ peer build deadline".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    for (path, sha) in &sources {
        if digest(&options.root.join(path))? != *sha {
            return Err("original C++ peer source changed during build".into());
        }
    }
    for (name, expected) in &headers {
        if digest(&options.root.join("msgq_repo/msgq").join(name))? != *expected {
            return Err("original C++ header changed during peer build".into());
        }
    }
    if digest(&options.compiler)? != compiler_sha256 {
        return Err("C++ compiler changed during peer build".into());
    }
    let argv = argv
        .iter()
        .map(|value| {
            value
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| "C++ peer argv must be UTF-8".into())
        })
        .collect::<Result<Vec<_>>>()?;
    let proof = json!({"kind":"direct-original-cpp-msgq-peer","argv":argv,"binary_sha256":digest(&binary)?,"source_sha256":sources,"header_sha256":headers,"services_header_sha256":digest(&header)?,"compiler":options.compiler,"compiler_sha256":compiler_sha256,"original_binding_builder_sha256":digest(&options.root.join("rust/tools/build_msgq_python.py"))?});
    Ok((binary, proof))
}

struct CompletedCase {
    records: Vec<Record>,
    timing: Vec<frame_loop::Timing>,
    command: Vec<String>,
    status: i32,
}

fn run_case(
    options: &Options,
    corpus: &Corpus,
    peer_binary: &Path,
    mode: &str,
    epoch: usize,
    prefix: &str,
    deadline: Instant,
) -> Result<(Vec<Record>, Value)> {
    let case = options.output.join(format!("{mode}-{epoch}"));
    fs::create_dir(&case)?;
    fs::create_dir(case.join("inputs"))?;
    let params = case.join("params").join(prefix);
    fs::create_dir_all(&params)?;
    for (name, raw) in &corpus.manifest.params {
        fs::write(params.join(name), corpus.read(raw)?)?;
    }
    let mut peer = Peer::start(
        peer_binary,
        prefix,
        &case.join("peer.stderr.log"),
        limit(deadline, 15),
    )?;
    let mut client = SchedulerClient::canonical_recorded(
        Some(&options.scheduler),
        &case.join("scheduler.stderr.log"),
        Some(&options.evidence),
    )?;
    let work = (|| -> Result<CompletedCase> {
        let command = if mode == "source" {
            vec![
                text(&options.python)?,
                "-P".into(),
                "rust/tools/plannerd_daemon_source.py".into(),
                "--binding".into(),
                text(&options.binding)?,
                "--source-native".into(),
                text(&options.source_native)?,
            ]
        } else {
            vec![
                text(&options.binary)?,
                "--solver".into(),
                text(&options.artifact)?,
            ]
        };
        let child = Command::new(&command[0])
            .args(&command[1..])
            .current_dir(&options.root)
            .env("OPENPILOT_PREFIX", prefix)
            .env("PARAMS_ROOT", params.parent().ok_or("missing Params root")?)
            .env("OPENPILOT_ROOT", &options.root)
            .env("PLANNER_ACADOS", &options.artifact)
            .env("SIMULATION", "0")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("OPENBLAS_NUM_THREADS", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(fs::File::create(case.join("stdout.log"))?))
            .stderr(Stdio::from(fs::File::create(case.join("stderr.log"))?))
            .spawn()?;
        let mut daemon = ChildOwner {
            child,
            reaped: false,
        };
        let work = (|| -> Result<(Vec<Record>, Vec<frame_loop::Timing>)> {
            pause(0.15, deadline)?;
            daemon.running()?;
            fs::write(
                params.join("CarParams.pending"),
                corpus.read(&corpus.manifest.car_params)?,
            )?;
            fs::rename(params.join("CarParams.pending"), params.join("CarParams"))?;
            let ready = limit(deadline, 15);
            loop {
                daemon.running()?;
                let response = peer.request("readers", ready)?;
                if response == "READERS 1" {
                    break;
                }
                if response != "READERS 0" {
                    return Err("invalid C++ readiness response".into());
                }
                pause(0.01, ready)?;
            }
            peer.ok("subscribe", limit(deadline, 2))?;
            let initial = stage(corpus, &corpus.manifest.initial_live, &case)?;
            peer.publish("liveTracks", &initial, limit(deadline, 2))?;
            let mut events = Events {
                peer: &mut peer,
                daemon: &mut daemon,
                corpus,
                case: &case,
                records: Vec::new(),
                deadline,
            };
            pause(0.12, deadline)?;
            events.drain(limit(deadline, 2))?;
            if !events.records.is_empty() {
                return Err("a liveTracks-only startup published without a model".into());
            }
            let timing = drive40(&mut client, &mut events, now() + 0.05)?;
            pause(0.03, deadline)?;
            events.drain(limit(deadline, 2))?;
            Ok((events.records, timing))
        })();
        let ((records, timing), status) = daemon.finish_daemon(work)?;
        Ok(CompletedCase {
            records,
            timing,
            command,
            status,
        })
    })();
    let work = client.finish(work);
    let work = peer.finish(work);
    corpus.verify()?;
    let CompletedCase {
        mut records,
        timing,
        command,
        status,
    } = work?;
    records.sort_by_key(|row| row.timestamp);
    let valid = OUTPUTS
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                records
                    .iter()
                    .filter(|row| row.service == *name && row.valid)
                    .count(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let triggers = ["modelV2", "liveTracks"]
        .map(|name| {
            (
                name.to_owned(),
                records
                    .iter()
                    .filter(|row| {
                        row.service == "longitudinalPlan" && row.data["planningTrigger"] == name
                    })
                    .count(),
            )
        })
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let overlays = records
        .iter()
        .filter(|row| {
            row.service == "longitudinalPlan"
                && row.data["fastLeadMask"]
                    .as_u64()
                    .is_some_and(|mask| mask != 0)
        })
        .count();
    write_json(
        &case.join("receipt.json"),
        &json!({"command":command,"returncode":status,"records":&records,"timing":&timing,"valid_counts":&valid,"triggers":&triggers,"fast_overlays":overlays}),
    )?;
    let summary = json!({"returncode":status,"messages":records.len(),"valid_counts":valid,"triggers":triggers,"fast_overlays":overlays});
    Ok((records, summary))
}

/// 실제 callable driver다. 부모 CLI/plan 연결과 독립 입력 capture 전에는 등록/수용하지 않는다.
pub fn run(options: &Options) -> Result<Value> {
    let capture = Capture::open(
        &options.corpus,
        &options.corpus_audit,
        &options.corpus_audit_sha256,
    )?;
    let original_outputs = OriginalOutputs::open(&options.corpus, &capture)?;
    let identities = [
        options.binary.clone(),
        options.binding.clone(),
        options.original_ipc_binding.clone(),
        options.artifact.join("manifest.json"),
    ]
    .iter()
    .map(|path| Ok((text(path)?, digest(path)?)))
    .collect::<Result<BTreeMap<_, _>>>()?;
    capture.verify_schema(&options.root)?;
    let work = run_authenticated(options, &capture, &original_outputs, &identities);
    // 역사 원문과 현재 실행물의 별도 봉인을 성공/실패 어느 반환 전에도 재검사한다.
    capture.verify()?;
    original_outputs.verify()?;
    capture.verify_schema(&options.root)?;
    for (path, expected) in &identities {
        if digest(Path::new(path))? != *expected {
            return Err("current Planner IPC binding/artifact/binary changed during run".into());
        }
    }
    work
}

fn run_authenticated(
    options: &Options,
    capture: &Capture,
    original_outputs: &OriginalOutputs,
    identities: &BTreeMap<String, String>,
) -> Result<Value> {
    if options.scheduler.canonicalize()? != std::env::current_exe()?.canonicalize()? {
        return Err(
            "native IPC must consume the current check-plannerd-ci schedule endpoint".into(),
        );
    }
    if options.timeout.is_zero()
        || options.growth_bytes == 0
        || options.peer_build_growth_bytes == 0
    {
        return Err("IPC timeout and operation growth estimates are required".into());
    }
    for path in [
        &options.root,
        &options.python,
        &options.binary,
        &options.artifact,
        &options.binding,
        &options.source_native,
        &options.original_ipc_binding,
        &options.scheduler,
        &options.compiler,
        &options.output,
        &options.evidence,
    ] {
        if !path.is_absolute() {
            return Err("IPC driver requires admitted absolute paths".into());
        }
    }
    let module_directory = options
        .original_ipc_binding
        .parent()
        .ok_or("missing original IPC package")?;
    if original_ipc_binding(module_directory)? != options.original_ipc_binding {
        return Err("original IPC binding selection differs".into());
    }
    let original_python_root = module_directory
        .parent()
        .ok_or("missing original IPC PYTHONPATH root")?;
    let pythonpath = std::env::var_os("PYTHONPATH").ok_or("original IPC PYTHONPATH is required")?;
    if std::env::split_paths(&pythonpath).next().as_deref() != Some(original_python_root) {
        return Err("original isolated C++ IPC module must be first on PYTHONPATH".into());
    }
    // 원래 Python C++ binding의 실제 hash 계약을 유지한다. 새 C++ 실행물과 혼동하지 않는다.
    if fs::symlink_metadata(&options.evidence).is_ok() {
        return Err("new IPC scheduler evidence path required".into());
    }
    openpilot_disk_space::inspect(
        options.output.parent().ok_or("missing IPC output parent")?,
        options.growth_bytes,
    )?
    .require()?;
    fs::create_dir(&options.output)?;
    let deadline = Instant::now() + options.timeout;
    let (peer_binary, mut proof) = build_peer(options, deadline)?;
    proof["original_cpp_binding"] = json!({"path":options.original_ipc_binding,"sha256":digest(&options.original_ipc_binding)?});
    proof["independent_original_outputs"] = original_outputs.provenance();
    proof["input_corpus"] = json!({
        "audit_sha256": capture.audit_sha256(),
        "external_audit": capture.audit(),
        "manifests": capture.index().runs.iter().map(|run| json!({
            "mode": &run.mode, "epoch": run.epoch, "manifest": &run.manifest
        })).collect::<Vec<_>>()
    });
    write_json(&options.output.join("native-peer-provenance.json"), &proof)?;
    // client/server만 증거를 쓴다. 160행을 driver에서 합성하지 않는다.
    let mut results = Vec::new();
    let mut summaries = Vec::new();
    for mode in ["source", "rust"] {
        let shared = tempfile::Builder::new()
            .prefix("msgq_planner_")
            .tempdir_in("/dev/shm")?;
        let name = shared
            .path()
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("invalid IPC namespace")?;
        let prefix = name.strip_prefix("msgq_").ok_or("invalid IPC prefix")?;
        for epoch in 0..2 {
            let corpus = capture.select(mode, epoch)?;
            let (records, summary) =
                run_case(options, corpus, &peer_binary, mode, epoch, prefix, deadline)?;
            results.push(records);
            summaries.push(summary);
        }
    }
    let comparisons=results.iter().enumerate().skip(1).map(|(index,records)|json!({"run":index,"differences":decode::differences(&results[0],records)})).collect::<Vec<_>>();
    let original_comparison = original_outputs.compare(&results)?;
    let receipt = json!({"identities":&identities,"runs":&summaries,"comparisons":&comparisons});
    for (name, expected) in proof["source_sha256"]
        .as_object()
        .ok_or("missing C++ source roster")?
    {
        if digest(&options.root.join(name))? != expected.as_str().ok_or("invalid C++ source SHA")? {
            return Err("C++ source changed during IPC run".into());
        }
    }
    for (name, expected) in proof["header_sha256"]
        .as_object()
        .ok_or("missing C++ header roster")?
    {
        if digest(&options.root.join("msgq_repo/msgq").join(name))?
            != expected.as_str().ok_or("invalid C++ header SHA")?
        {
            return Err("C++ header changed during IPC run".into());
        }
    }
    if digest(&peer_binary)?
        != proof["binary_sha256"]
            .as_str()
            .ok_or("missing C++ peer SHA")?
    {
        return Err("C++ peer changed during IPC run".into());
    }
    capture.verify()?;
    original_outputs.verify()?;
    scheduler::verify_evidence(&options.evidence, 160)?;
    write_json(&options.output.join("receipt.json"), &receipt)?;
    write_json(
        &options.output.join("original-output-comparison.json"),
        &original_comparison,
    )?;
    if comparisons.iter().any(|row| {
        row["differences"]
            .as_array()
            .is_none_or(|rows| !rows.is_empty())
    }) {
        return Err("Planner original/native IPC publications differ".into());
    }
    for row in &summaries {
        for field in ["valid_counts", "triggers"] {
            if row[field]
                .as_object()
                .ok_or("missing IPC coverage counters")?
                .values()
                .any(|count| count.as_u64().unwrap_or(0) == 0)
            {
                return Err("missing valid steady-state/trigger/overlay IPC coverage".into());
            }
        }
        if row["fast_overlays"].as_u64().unwrap_or(0) == 0 {
            return Err("missing trigger or overlay coverage".into());
        }
    }
    if original_comparison["passed"] != true {
        return Err("Planner IPC differs from authenticated original 121x4 outputs".into());
    }
    Ok(receipt)
}
