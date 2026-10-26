//! lifecycle13×source/rust의 실제 driver. 실제 수용은 같은 source의 부모 Actions에서 확인한다.
use super::{
    corpus::RawFile,
    decode,
    lifecycle_corpus::{Capture, CaseResult, HeldFile, Run, State, CASES, MIB},
    peer::Peer,
    ChildOwner, OUTPUTS, PEER_FILES,
};
use crate::seal::{digest, Result};
use rustix::process::{getpgrp, getpid, getuid, kill_process, Pid, Signal};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) struct Options {
    pub root: PathBuf,
    pub python: PathBuf,
    pub binary: PathBuf,
    pub artifact: PathBuf,
    pub binding: PathBuf,
    pub source_native: PathBuf,
    pub original_ipc_binding: PathBuf,
    pub corpus: PathBuf,
    pub corpus_audit: PathBuf,
    pub output: PathBuf,
    pub timeout: Duration,
    pub growth_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerProof {
    kind: String,
    argv: Vec<String>,
    binary_sha256: String,
    source_sha256: BTreeMap<String, String>,
    header_sha256: BTreeMap<String, String>,
    services_header_sha256: String,
    compiler: PathBuf,
    compiler_sha256: String,
    original_binding_builder_sha256: String,
    original_cpp_binding: Binding,
    independent_original_outputs: Value,
    input_corpus: Value,
}
struct RuntimePeer {
    binary: PathBuf,
    held: Vec<HeldFile>,
    compiler_alias: PathBuf,
    compiler_target: PathBuf,
    python_alias: PathBuf,
    python_target: PathBuf,
}
impl RuntimePeer {
    fn read(
        &mut self,
        path: &Path,
        pin: Option<&str>,
        limit: u64,
        retain: bool,
    ) -> Result<Vec<u8>> {
        let size = fs::symlink_metadata(path)?.len();
        if size > limit || path.canonicalize()? != path {
            return Err("noncanonical/bounded lifecycle runtime input required".into());
        }
        let sha = digest(path)?;
        if pin.is_some_and(|v| v != sha) {
            return Err("current lifecycle runtime identity differs".into());
        }
        let raw = RawFile {
            path: path
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or("runtime filename")?
                .into(),
            bytes: size,
            sha256: sha,
        };
        let (bytes, held) = HeldFile::open(
            path.parent().ok_or("runtime file parent")?,
            &raw,
            limit,
            retain,
        )?;
        self.held.push(held);
        Ok(bytes)
    }
    fn open(options: &Options) -> Result<Self> {
        let job = PathBuf::from(std::env::var("CI_JOB_TEMP")?);
        let runtime = job.join("planner-comparison");
        let owner = format!(
            "{}:{}:{}",
            std::env::var("GITHUB_RUN_ID")?,
            std::env::var("GITHUB_RUN_ATTEMPT")?,
            std::env::var("GITHUB_JOB")?
        );
        if getpid() != getpgrp()
            || std::env::var("GITHUB_ACTIONS")? != "true"
            || std::env::var("GITHUB_REPOSITORY")? != "bin9208/openpilot-rust"
            || std::env::var("GITHUB_JOB")? != "planner-runtime"
            || job.parent() != Some(Path::new("/tmp"))
            || !job.file_name().and_then(|v| v.to_str()).is_some_and(|v| {
                v.strip_prefix("openpilot-ci.")
                    .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
            })
            || options.output != runtime.join("lifecycle")
            || options.corpus != job.join("planner-lifecycle-original")
            || options.corpus_audit != job.join("planner-lifecycle-original/audit.json")
            || options.root != PathBuf::from(std::env::var("CI296_SOURCE")?)
        {
            return Err("lifecycle requires the same isolated owned runtime worker/job".into());
        }
        for directory in [&job, &runtime, &options.corpus] {
            let metadata = fs::symlink_metadata(directory)?;
            if directory.canonicalize()? != *directory
                || !metadata.is_dir()
                || metadata.uid() != getuid().as_raw()
                || metadata.permissions().mode() & 0o777 != 0o700
            {
                return Err("lifecycle owned mode700 root required".into());
            }
        }
        let ipc = runtime.join("ipc");
        let mut current = Self {
            binary: ipc.join("planner-ipc-cpp-peer"),
            held: Vec::new(),
            compiler_alias: PathBuf::new(),
            compiler_target: PathBuf::new(),
            python_alias: options.python.clone(),
            python_target: options.python.canonicalize()?,
        };
        if current.read(&job.join("owner"), None, 512, true)? != owner.as_bytes() {
            return Err("lifecycle job owner differs".into());
        }
        let source = std::env::var("GITHUB_SHA")?;
        let host = std::env::var("NATIVE_TOOLS_HOST")?;
        let (expected_host, seconds) = match std::env::consts::ARCH {
            "x86_64" => ("x86_64-unknown-linux-gnu", 2700),
            "aarch64" => ("aarch64-unknown-linux-gnu", 5400),
            _ => return Err("unsupported lifecycle native host".into()),
        };
        if host != expected_host
            || std::env::consts::OS != "linux"
            || options.timeout != Duration::from_secs(seconds)
        {
            return Err("lifecycle native host/original timeout differs".into());
        }
        let profile = job.join("proof/planner-native-controller-523");
        if current.read(&profile.join("controller-source.txt"), None, MIB, true)?
            != format!("{source}:{host}\n").as_bytes()
            || current.read(&profile.join("controller-profile.txt"), None, MIB, true)?
                != b"ci-receipt-metadata-native,planner-ipc-native\n"
        {
            return Err("lifecycle controller source/profile differs".into());
        }
        let controller = std::env::current_exe()?.canonicalize()?;
        let controller_sha = digest(&controller)?;
        if current.read(&profile.join("controller.sha256"), None, MIB, true)?
            != format!("{controller_sha}  {}\n", controller.display()).as_bytes()
        {
            return Err("lifecycle must consume the qualified controller ELF".into());
        }
        current.read(&controller, Some(&controller_sha), 512 * MIB, false)?;
        current.read(&profile.join("sdk.before.sha256"), None, MIB, false)?;
        let commands: Vec<Value> = serde_json::from_slice(&current.read(
            &runtime.join("commands.json"),
            None,
            MIB,
            true,
        )?)?;
        if commands.len() != 11
            || commands
                .iter()
                .zip(crate::planner_ci_controller::plan::CHECK_NAMES)
                .any(|(row, name)| row["name"] != name || row["returncode"] != 0)
            || commands[10]["argv"][0].as_str() != controller.to_str()
            || commands[10]["argv"][1] != "ipc"
        {
            return Err(
                "lifecycle requires the same runtime's preceding successful IPC leaf".into(),
            );
        }
        let argv = commands[10]["argv"]
            .as_array()
            .ok_or("preceding IPC argv")?;
        for (flag, path) in [
            ("--root", &options.root),
            ("--python", &options.python),
            ("--binary", &options.binary),
            ("--artifact", &options.artifact),
            ("--binding", &options.binding),
            ("--source-native", &options.source_native),
            ("--original-ipc-binding", &options.original_ipc_binding),
            ("--scheduler", &controller),
            ("--output", &ipc),
        ] {
            if argv
                .windows(2)
                .find(|pair| pair[0] == flag)
                .and_then(|pair| pair[1].as_str())
                != path.to_str()
            {
                return Err("lifecycle predecessor IPC input/output role differs".into());
            }
        }
        if options.python != job.join("venv/bin/python")
            || current.read(&profile.join("runtime-python.txt"), None, MIB, true)?
                != format!("{}\n", options.python.display()).as_bytes()
        {
            return Err("lifecycle must retain the runtime's Python venv invocation".into());
        }
        let python = current.python_target.clone();
        current.read(&python, None, 512 * MIB, false)?;
        super::scheduler::verify_evidence(&runtime.join("ipc-scheduler.tsv"), 160)?;
        current.read(&runtime.join("ipc-scheduler.tsv"), None, MIB, false)?;
        let proof: PeerProof = serde_json::from_slice(&current.read(
            &ipc.join("native-peer-provenance.json"),
            None,
            MIB,
            true,
        )?)?;
        if proof.kind != "direct-original-cpp-msgq-peer"
            || proof.source_sha256.len() != 6
            || proof.header_sha256.len() != 5
            || proof.original_cpp_binding.path != options.original_ipc_binding
            || proof.argv.first().map(PathBuf::from).as_ref() != Some(&proof.compiler)
            || proof.independent_original_outputs["original_packets_decoded"] != 484
            || !proof.input_corpus.is_object()
        {
            return Err("lifecycle previous peer provenance/role differs".into());
        }
        let original_comparison: Value = serde_json::from_slice(&current.read(
            &ipc.join("original-output-comparison.json"),
            None,
            MIB,
            true,
        )?)?;
        if original_comparison["passed"] != true
            || original_comparison["provenance"]["original_packets_decoded"] != 484
        {
            return Err("lifecycle predecessor original output comparison did not pass".into());
        }
        for name in PEER_FILES {
            current.read(
                &options.root.join(name),
                Some(proof.source_sha256.get(name).ok_or("peer source missing")?),
                MIB,
                false,
            )?;
        }
        let mut argv = vec![
            super::text(&proof.compiler)?,
            "-std=c++17".into(),
            "-UNDEBUG".into(),
            "-pthread".into(),
            format!("-I{}", options.root.join("msgq_repo").display()),
            format!("-I{}", ipc.join("peer-include").display()),
        ];
        for name in PEER_FILES {
            argv.push(super::text(&options.root.join(name))?);
        }
        argv.extend(["-o".into(), super::text(&current.binary)?]);
        if proof.argv != argv {
            return Err("lifecycle predecessor C++17 argv differs".into());
        }
        for name in ["ipc.h", "event.h", "impl_msgq.h", "impl_fake.h", "msgq.h"] {
            current.read(
                &options.root.join("msgq_repo/msgq").join(name),
                Some(proof.header_sha256.get(name).ok_or("peer header missing")?),
                MIB,
                false,
            )?;
        }
        current.read(
            &ipc.join("peer-include/services.h"),
            Some(&proof.services_header_sha256),
            MIB,
            false,
        )?;
        current.read(
            &options.root.join("rust/tools/build_msgq_python.py"),
            Some(&proof.original_binding_builder_sha256),
            MIB,
            false,
        )?;
        current.read(
            &options.original_ipc_binding,
            Some(&proof.original_cpp_binding.sha256),
            512 * MIB,
            false,
        )?;
        current.compiler_alias = proof.compiler;
        current.compiler_target = current.compiler_alias.canonicalize()?;
        let compiler = current.compiler_target.clone();
        current.read(&compiler, Some(&proof.compiler_sha256), 512 * MIB, false)?;
        let binary = current.binary.clone();
        current.read(&binary, Some(&proof.binary_sha256), 512 * MIB, false)?;
        for path in [
            &options.binary,
            &options.binding,
            &options.artifact.join("manifest.json"),
        ] {
            current.read(path, None, 512 * MIB, false)?;
        }
        for kind in ["lateral", "longitudinal"] {
            current.read(
                &options
                    .source_native
                    .join(kind)
                    .join("c_generated_code/acados_ocp_solver_pyx.so"),
                None,
                512 * MIB,
                false,
            )?;
        }
        for name in [
            "rust/tools/plannerd_daemon_source.py",
            "rust/tools/plannerd_owner_loader.py",
            "openpilot/selfdrive/controls/plannerd.py",
        ] {
            current.read(&options.root.join(name), None, MIB, false)?;
        }
        current.verify()?;
        Ok(current)
    }
    fn verify(&self) -> Result<()> {
        for file in &self.held {
            file.verify()?;
        }
        if self.compiler_alias.canonicalize()? != self.compiler_target {
            return Err("lifecycle compiler alias changed".into());
        }
        if self.python_alias.canonicalize()? != self.python_target {
            return Err("lifecycle Python venv target changed".into());
        }
        Ok(())
    }
}

pub(super) fn poll(child: &mut ChildOwner) -> Result<Option<i32>> {
    let status = child.child.try_wait()?;
    if status.is_some() {
        child.reaped = true;
    }
    Ok(status.map(|s| s.code().unwrap_or_else(|| -s.signal().unwrap_or(1))))
}
pub(super) fn wait(child: &mut ChildOwner, deadline: Instant) -> Result<i32> {
    loop {
        if let Some(code) = poll(child)? {
            return Ok(code);
        }
        super::pause(0.001, deadline)?;
    }
}
pub(super) fn signal(child: &mut ChildOwner, value: i32) -> Result<()> {
    // Popen.send_signal처럼 이미 끝난 child와 poll/kill 사이의 ESRCH는 wait의 raw status로 처리한다.
    if poll(child)?.is_some() {
        return Ok(());
    }
    let pid = Pid::from_raw(child.child.id().try_into()?).ok_or("invalid lifecycle daemon PID")?;
    match kill_process(
        pid,
        if value == 2 {
            Signal::INT
        } else {
            Signal::TERM
        },
    ) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn spawn_daemon(command: &mut Command) -> Result<ChildOwner> {
    openpilot_process_limits::disable_core_dump(command);
    Ok(ChildOwner {
        child: command.spawn()?,
        reaped: false,
    })
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}
fn stage_params(capture: &Capture, run: &Run, params: &Path) -> Result<()> {
    for (key, state) in &run.params["prelaunch"].entries {
        match state {
            State::Absent { .. } => {}
            State::Directory { .. } => fs::create_dir(params.join(key))?,
            State::File { raw } => write_new(&params.join(key), &capture.bytes(raw)?)?,
        }
    }
    Ok(())
}
fn recovery(capture: &Capture, run: &Run, params: &Path) -> Result<()> {
    // 원본처럼 CP directory만 rmdir/write한다. 다른 Params에 새 값 동일성 조건을 더하지 않는다.
    let State::File { raw } = &run.params["recovery-after"].entries["CarParams"] else {
        return Err("missing observed recovery CP bytes".into());
    };
    fs::remove_dir(params.join("CarParams"))?;
    write_new(&params.join("CarParams"), &capture.bytes(raw)?)
}
fn waiting(child: &mut ChildOwner, output: &Path, overall: Instant) -> Result<()> {
    let deadline = super::limit(overall, 15);
    loop {
        child.running()?;
        if ["stdout.log", "stderr.log"]
            .iter()
            .map(|name| fs::read_to_string(output.join(name)))
            .collect::<std::io::Result<Vec<_>>>()?
            .iter()
            .any(|text| text.contains("plannerd is waiting for CarParams"))
        {
            super::pause(0.12, overall)?;
            child.running()?;
            return Ok(());
        }
        super::pause(0.01, deadline)?;
    }
}
fn frames(
    capture: &Capture,
    run: &Run,
    peer: &mut Peer,
    child: &mut ChildOwner,
    output: &Path,
    overall: Instant,
) -> Result<Vec<String>> {
    let mut seen = Vec::new();
    for frame in 0..run.inputs.len() / 9 {
        for (n, input) in run.inputs[frame * 9..(frame + 1) * 9].iter().enumerate() {
            let path =
                output
                    .join("inputs")
                    .join(format!("{}-{}.bin", frame * 9 + n, input.service));
            write_new(&path, &capture.bytes(&input.raw)?)?;
            peer.publish(&input.service, &path, overall)?;
        }
        if run.case == "fatal-float" {
            return Ok(seen);
        }
        let deadline = super::limit(overall, 3);
        let mut services = BTreeSet::new();
        while services.len() != OUTPUTS.len() {
            child.running()?;
            for (service, raw) in peer.drain(output, deadline)? {
                if !OUTPUTS.contains(&service.as_str()) {
                    return Err("unexpected lifecycle output service".into());
                }
                let size = fs::symlink_metadata(&raw)?;
                if !size.is_file() || size.file_type().is_symlink() || size.len() > 16 * MIB {
                    return Err("invalid lifecycle publication file".into());
                }
                let decoded =
                    decode::packet(&fs::read(&raw)?, &service, raw.to_string_lossy().into())?;
                if decoded.service != service {
                    return Err("lifecycle publication union differs".into());
                }
                fs::rename(
                    raw,
                    output.join(format!("publication-{frame}-{}-{service}.bin", seen.len())),
                )?;
                services.insert(service.clone());
                seen.push(service);
            }
            super::pause(0.001, deadline)?;
        }
        super::pause(0.05, overall)?;
    }
    Ok(seen)
}

fn run_case(
    options: &Options,
    capture: &Capture,
    run: &Run,
    peer_binary: &Path,
    overall: Instant,
) -> Result<CaseResult> {
    let case = CASES
        .iter()
        .find(|v| v.name == run.case)
        .ok_or("unknown lifecycle case")?;
    let output = options.output.join(&run.case).join(&run.mode);
    fs::create_dir_all(&output)?;
    fs::create_dir(output.join("inputs"))?;
    let shared = tempfile::Builder::new()
        .prefix("msgq_planner_fault_")
        .tempdir_in("/dev/shm")?;
    let prefix = shared
        .path()
        .file_name()
        .and_then(|v| v.to_str())
        .and_then(|v| v.strip_prefix("msgq_"))
        .ok_or("lifecycle namespace")?;
    let mut peer = Peer::start(
        peer_binary,
        prefix,
        &output.join("peer.stderr.log"),
        super::limit(overall, 15),
    )?;
    let params = output.join("params").join(prefix);
    fs::create_dir_all(&params)?;
    stage_params(capture, run, &params)?;
    let argv: Vec<String> = if run.mode == "source" {
        vec![
            super::text(&options.python)?,
            "-u".into(),
            "-P".into(),
            super::text(&options.root.join("rust/tools/plannerd_daemon_source.py"))?,
            "--binding".into(),
            super::text(&options.binding)?,
            "--source-native".into(),
            super::text(&options.source_native)?,
            "--log-root".into(),
            super::text(&output.join("logs"))?,
        ]
    } else {
        vec![
            super::text(&options.binary)?,
            "--solver".into(),
            super::text(&options.artifact)?,
        ]
    };
    super::write_json(
        &output.join("invocation.json"),
        &json!({"argv":argv,"prefix":prefix}),
    )?;
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(&options.root)
        .env("OPENPILOT_PREFIX", prefix)
        .env("PARAMS_ROOT", params.parent().ok_or("Params root")?)
        .env("PWD", &options.root)
        .env("LOGPRINT", "info")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("OPENBLAS_NUM_THREADS", "1")
        .env("PYTHONUNBUFFERED", "1")
        .env("SIMULATION", "0")
        .stdout(Stdio::from(fs::File::create(output.join("stdout.log"))?))
        .stderr(Stdio::from(fs::File::create(output.join("stderr.log"))?));
    // 새 group/session/NativeRunner를 만들지 않는다. 직접 child와 peer 모두 outer worker PGID를 상속한다.
    let mut daemon = spawn_daemon(&mut command)?;
    let mut result = CaseResult {
        case: run.case.clone(),
        mode: run.mode.clone(),
        ok: false,
        publications: None,
        returncode: None,
        expected_returncode: None,
        error: None,
    };
    let work = (|| -> Result<()> {
        if matches!(case.phase, "wait" | "recover") {
            waiting(&mut daemon, &output, overall)?;
        }
        if case.phase == "recover" {
            recovery(capture, run, &params)?;
        }
        if matches!(case.phase, "ready" | "recover" | "float") {
            let deadline = super::limit(overall, 15);
            loop {
                daemon.running()?;
                match peer.request("readers", deadline)?.as_str() {
                    "READERS 1" => break,
                    "READERS 0" => super::pause(0.01, deadline)?,
                    _ => return Err("invalid lifecycle readers reply".into()),
                }
            }
            if case.phase != "float" {
                peer.ok("subscribe", overall)?;
            }
            result.publications = Some(frames(
                capture,
                run,
                &mut peer,
                &mut daemon,
                &output,
                overall,
            )?);
        }
        let fatal = matches!(case.phase, "integer" | "float" | "malformed");
        if !fatal {
            signal(&mut daemon, case.signal)?;
        }
        result.returncode = Some(wait(
            &mut daemon,
            super::limit(overall, if fatal { 15 } else { 5 }),
        )?);
        result.expected_returncode = Some(case.expected(&run.mode));
        if result.returncode != result.expected_returncode {
            return Err("lifecycle raw source/native returncode differs".into());
        }
        if Path::new("/proc")
            .join(daemon.child.id().to_string())
            .exists()
        {
            return Err("lifecycle direct child was not reaped".into());
        }
        Ok(())
    })();
    let mut cleanup = Vec::new();
    if let Err(error) = work {
        result.error = Some(error.to_string());
        match poll(&mut daemon) {
            Ok(code) => result.returncode = code,
            Err(error) => cleanup.push(error.to_string()),
        }
    }
    if !daemon.reaped {
        let killed = daemon.child.kill();
        let closed = wait(&mut daemon, Instant::now() + Duration::from_secs(5));
        if let (true, Err(error)) = (closed.is_err(), killed) {
            cleanup.push(error.to_string());
        }
        if let Err(error) = closed {
            cleanup.push(error.to_string());
            if result.error.is_none() {
                result.error = Some(error.to_string());
            }
        }
    }
    let prior = result.error.clone();
    let closed = peer.finish(prior.as_ref().map_or(Ok(()), |v| Err(v.clone().into())));
    if let Err(error) = closed {
        if prior.as_deref() != Some(error.to_string().as_str()) {
            cleanup.push(error.to_string());
        }
        if result.error.is_none() {
            result.error = Some(error.to_string());
        }
    }
    if !cleanup.is_empty() {
        super::write_json(&output.join("cleanup-errors.json"), &cleanup)
            .unwrap_or_else(|error| eprintln!("lifecycle cleanup diagnostic: {error}"));
    }
    result.ok = result.error.is_none();
    super::write_json(&output.join("receipt.json"), &result)?;
    Ok(result)
}

pub(crate) fn run(options: &Options) -> Result<Value> {
    // 승인된 audit 확인은 output 생성/peer/daemon spawn보다 먼저다.
    let capture = Capture::open(&options.corpus, &options.corpus_audit)?;
    let peer = RuntimePeer::open(options)?;
    if options.timeout.is_zero() || options.growth_bytes < 128 * MIB {
        return Err("lifecycle timeout/growth admission required".into());
    }
    openpilot_disk_space::inspect(
        options.output.parent().ok_or("lifecycle output parent")?,
        options.growth_bytes,
    )?
    .require()?;
    fs::create_dir(&options.output)?;
    let deadline = Instant::now()
        .checked_add(options.timeout)
        .ok_or("lifecycle deadline range")?;
    let work = (|| -> Result<Vec<CaseResult>> {
        let mut results = Vec::new();
        for row in &capture.runs {
            let result = run_case(options, &capture, row, &peer.binary, deadline)?;
            println!(
                "{} {} {}",
                result.case,
                result.mode,
                if result.ok { "True" } else { "False" }
            );
            std::io::stdout().flush()?;
            results.push(result);
            super::write_json(&options.output.join("cases.json"), &results)?;
        }
        Ok(results)
    })();
    let sealed = capture.verify().and_then(|()| peer.verify());
    if let Err(error) = &sealed {
        super::write_json(
            &options.output.join("postflight-error.json"),
            &error.to_string(),
        )
        .unwrap_or_else(|diagnostic| eprintln!("lifecycle postflight diagnostic: {diagnostic}"));
    }
    let results = work?;
    sealed?;
    let receipt = json!({"status":if results.iter().all(|v| v.ok) {"PASS"} else {"FAIL"},"cases":13,
        "binary_sha256":digest(&options.binary)?,"scope":"real source/native lifecycle; native clean SIGINT matches the original manager launcher",
        "exit_boundary":"raw source KeyboardInterrupt=-2; invalid numeric source abort vs typed Rust exit1 remains explicit"});
    super::write_json(&options.output.join("receipt.json"), &receipt)?;
    super::write_json(
        &options.output.join("native-lifecycle-provenance.json"),
        &json!({"input_audit_sha256":capture.audit_sha256,
        "peer_sha256":digest(&peer.binary)?,"controller":std::env::current_exe()?,"retirement_credit":0}),
    )?;
    if receipt["status"] != "PASS" {
        return Err("planner lifecycle mismatches; see individual receipts".into());
    }
    Ok(receipt)
}
