//! New-mode cases only. No legacy resume or old C++ provider is used here.
use super::*;
use openpilot_dev_tools::seal::digest;
use std::{
    ffi::OsString,
    os::{fd::AsRawFd, unix::fs::PermissionsExt},
};

fn fixed_git_preload_environment(
    command: &mut Command,
    values: &std::collections::BTreeMap<OsString, OsString>,
) {
    let fixed = [
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "core.preloadIndex"),
        ("GIT_CONFIG_VALUE_0", "false"),
    ];
    // 이 실험의 정확한 schema만 전달한다. 누락을 기본 true로 숨기지 않는다.
    for (key, expected) in fixed {
        assert_eq!(
            values
                .get(std::ffi::OsStr::new(key))
                .map(OsString::as_os_str),
            Some(std::ffi::OsStr::new(expected)),
            "retained fixture requires fixed #554 {key}"
        );
    }
    command.env_clear();
    for name in ["PATH", "HOME", "TMPDIR", "TZ", "LANG", "LC_ALL"] {
        if let Some(value) = values.get(std::ffi::OsStr::new(name)) {
            command.env(name, value);
        }
    }
    command.envs(fixed);
}

#[test]
fn fixed_git_preload_environment_rejects_missing_or_mutated_schema() {
    use std::collections::BTreeMap;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let selected = [
        ("PATH", "/usr/bin:/bin"),
        ("HOME", "/private-home"),
        ("TMPDIR", "/private-tmp"),
        ("TZ", "UTC"),
        ("LANG", "C.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "core.preloadIndex"),
        ("GIT_CONFIG_VALUE_0", "false"),
    ];
    let expected: BTreeMap<OsString, OsString> = selected
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
    let mut supplied = expected.clone();
    for key in [
        "GIT_CONFIG_KEY_1",
        "GIT_CONFIG_VALUE_1",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_SSH_COMMAND",
    ] {
        supplied.insert(key.into(), "must-not-inherit".into());
    }
    let mut command = Command::new(CHECKER);
    command.env("GIT_CONFIG_PARAMETERS", "must-be-cleared");
    fixed_git_preload_environment(&mut command, &supplied);
    let transmitted: BTreeMap<OsString, OsString> = command
        .get_envs()
        .map(|(key, value)| (key.to_owned(), value.unwrap().to_owned()))
        .collect();
    assert_eq!(transmitted, expected);
    for (key, mutation) in [
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_0", "true"),
    ] {
        for replacement in [None, Some(mutation)] {
            let mut invalid = expected.clone();
            invalid.remove(std::ffi::OsStr::new(key));
            if let Some(value) = replacement {
                invalid.insert(key.into(), value.into());
            }
            assert!(catch_unwind(AssertUnwindSafe(|| {
                fixed_git_preload_environment(&mut Command::new(CHECKER), &invalid);
            }))
            .is_err());
        }
    }
    assert!(catch_unwind(AssertUnwindSafe(|| {
        fixed_git_preload_environment(&mut Command::new(CHECKER), &BTreeMap::new());
    }))
    .is_err());
    // 이 검사는 command metadata만 검사한다. checker/Git/원문을 실행하지 않는다.
}

struct Inputs {
    binary: PathBuf,
    context: PathBuf,
    source: PathBuf,
    commit: String,
    binary_sha256: String,
    context_sha256: String,
}
impl Inputs {
    fn actions() -> Self {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        for (name, expected) in [
            ("RUNNER_ENVIRONMENT", "github-hosted"),
            ("RUNNER_OS", "Linux"),
            ("RUNNER_ARCH", "X64"),
            ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ] {
            assert_eq!(std::env::var(name).as_deref(), Ok(expected), "{name}");
        }
        for key in [
            "NATIVE_LOGGING_LIFECYCLE_RESUME",
            "NATIVE_LOGGING_TEST_BUILDER",
            "NATIVE_LOGGING_TEST_REFERENCE",
        ] {
            assert!(
                std::env::var_os(key).is_none(),
                "retained fixture refuses {key}"
            );
        }
        let text = |name| std::env::var(name).unwrap_or_else(|_| panic!("Actions must set {name}"));
        let absolute = |name| {
            let path = PathBuf::from(
                std::env::var_os(name).unwrap_or_else(|| panic!("Actions must set {name}")),
            );
            assert!(path.is_absolute());
            path
        };
        let evidence = absolute("NATIVE_LOGGING_LIFECYCLE_EVIDENCE");
        let evidence_meta = fs::symlink_metadata(&evidence).unwrap();
        assert!(evidence_meta.is_dir() && !evidence_meta.file_type().is_symlink());
        assert_eq!(
            evidence_meta.uid(),
            fs::metadata("/proc/self").unwrap().uid()
        );
        assert_eq!(
            evidence_meta.mode() & 0o077,
            0,
            "retained evidence must be private0700"
        );
        let result = Self {
            binary: absolute("NATIVE_LOGGING_RETAINED_BINARY"),
            context: absolute("NATIVE_LOGGING_RETAINED_CONTEXT"),
            source: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .canonicalize()
                .unwrap(),
            commit: text("NATIVE_LOGGING_RETAINED_SOURCE_COMMIT"),
            binary_sha256: text("NATIVE_LOGGING_RETAINED_BINARY_SHA256"),
            context_sha256: text("NATIVE_LOGGING_RETAINED_CONTEXT_SHA256"),
        };
        for (value, length) in [
            (&result.commit, 40),
            (&result.binary_sha256, 64),
            (&result.context_sha256, 64),
        ] {
            assert_eq!(value.len(), length);
            assert!(value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        }
        // These pins bind the supplied data/provider, not their build provenance.
        // Parent must bind actual fresh build/DSO/dep-info/source closure separately.
        result.unchanged();
        result
    }
    fn unchanged(&self) {
        assert_eq!(digest(&self.binary).unwrap(), self.binary_sha256);
        assert_eq!(digest(&self.context).unwrap(), self.context_sha256);
    }
    fn command(&self, output: &Path) -> Command {
        let mut command = Command::new(CHECKER);
        fixed_git_preload_environment(&mut command, &std::env::vars_os().collect());
        command
            .args(["retained", "--root"])
            .arg(&self.source)
            .arg("--binary")
            .arg(&self.binary)
            .arg("--context")
            .arg(&self.context)
            .arg("--output")
            .arg(output)
            .arg("--source-commit")
            .arg(&self.commit)
            .arg("--binary-sha256")
            .arg(&self.binary_sha256);
        command
    }
}
fn argv(command: &Command) -> Vec<String> {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|value| {
            value
                .to_str()
                .expect("retained fixture argv must be UTF8")
                .to_owned()
        })
        .collect()
}
fn save(root: &Path, name: &str, value: Value) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
}
fn process_identity(pid: u32) -> Value {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields = text
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let uids = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .unwrap()
        .split_whitespace()
        .map(|uid| uid.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(uids.len(), 4);
    json!({"pid":pid,"uids":uids,"parent":fields[1].parse::<u32>().unwrap(),
        "group":fields[2].parse::<u32>().unwrap(),"session":fields[3].parse::<u32>().unwrap(),
        "start_ticks":fields[19].parse::<u64>().unwrap()})
}
// These observation helpers never signal. An OwnedFd in an ordinary local
// variable/vector only closes on failure; pidfd creation does not arm cleanup.
fn pidfd_receipt(fd: &OwnedFd, pid: u32) -> Value {
    let text = fs::read_to_string(format!("/proc/self/fdinfo/{}", fd.as_raw_fd())).unwrap();
    let target = text
        .lines()
        .find_map(|line| line.strip_prefix("Pid:"))
        .unwrap()
        .trim()
        .parse::<i64>()
        .unwrap();
    let namespace_pids = text
        .lines()
        .find_map(|line| line.strip_prefix("NSpid:"))
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse::<i64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(target, i64::from(pid));
    assert_eq!(namespace_pids.first(), Some(&i64::from(pid)));
    json!({"pid":target,"namespace_pids":namespace_pids})
}
fn executable_identity(metadata: &fs::Metadata) -> Value {
    json!({"dev":metadata.dev(),"ino":metadata.ino(),"uid":metadata.uid(),"mode":metadata.mode(),
        "size":metadata.len(),"mtime":metadata.mtime(),"mtime_nsec":metadata.mtime_nsec(),
        "ctime":metadata.ctime(),"ctime_nsec":metadata.ctime_nsec()})
}
fn observe_process(pid: u32, unarmed: &OwnedFd, executable: &Path, expected_sha256: &str) -> Value {
    let before = process_identity(pid);
    let receipt = pidfd_receipt(unarmed, pid);
    let uid = fs::metadata("/proc/self").unwrap().uid();
    assert!(before["uids"]
        .as_array()
        .unwrap()
        .iter()
        .all(|value| value.as_u64() == Some(u64::from(uid))));
    let expected = fs::metadata(executable).unwrap();
    let actual_path = PathBuf::from(format!("/proc/{pid}/exe"));
    let actual = fs::metadata(&actual_path).unwrap();
    assert!(expected.is_file() && actual.is_file());
    assert_eq!(expected.uid(), uid);
    assert_eq!(actual.uid(), uid);
    assert_eq!(
        (actual.dev(), actual.ino()),
        (expected.dev(), expected.ino())
    );
    let actual_identity = executable_identity(&actual);
    assert_eq!(digest(&actual_path).unwrap(), expected_sha256);
    assert_eq!(
        actual_identity,
        executable_identity(&fs::metadata(&actual_path).unwrap())
    );
    assert_eq!(
        executable_identity(&expected),
        executable_identity(&fs::metadata(executable).unwrap())
    );
    assert_eq!(receipt, pidfd_receipt(unarmed, pid));
    assert_eq!(before, process_identity(pid));
    json!({"live_identity":before,"pidfd_receipt":receipt,
        "executable":actual_identity,"sha256":expected_sha256})
}
fn guarded(child: Child, stderr: PathBuf) -> Checker {
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    Checker {
        child: Some(child),
        identity,
        peer: None,
        stderr,
    }
}
fn begin(
    inputs: &Inputs,
    mode: &str,
    escaped: bool,
) -> (PathBuf, PathBuf, Checker, Value, Vec<String>) {
    let root = root();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let output = root.join("output");
    let records = root.join("records");
    fs::create_dir(&records).unwrap();
    let stderr = root.join("checker.stderr");
    let mut command = inputs.command(&output);
    command
        .env("NATIVE_LOGGING_QA_HOLD", mode)
        .env("NATIVE_LOGGING_QA_RECORDS", &records);
    if escaped {
        assert_eq!(mode, "first");
        command.env("NATIVE_LOGGING_QA_ESCAPED", "1");
    }
    let actual_argv = argv(&command);
    let child = command
        .stdout(File::create(root.join("checker.stdout")).unwrap())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut checker = guarded(child, stderr);
    checker.wait(
        Duration::from_secs(if mode == "runtime" { 60 } else { 15 }),
        "retained actual native barrier",
        || records.join("held.json").is_file(),
    );
    let mut held: Value =
        serde_json::from_slice(&fs::read(records.join("held.json")).unwrap()).unwrap();
    let pid: i32 = held["pid"].as_u64().unwrap().try_into().unwrap();
    // The checker guard owns an actual spawned Child, independently of receipts.
    // The receipt-derived peer FD remains unarmed through every validation below.
    let checker_pid = checker.child.as_ref().unwrap().id();
    let checker_sha256 = digest(Path::new(CHECKER)).unwrap();
    let checker_before = observe_process(
        checker_pid,
        &checker.identity,
        Path::new(CHECKER),
        &checker_sha256,
    );
    let owner = process_identity(std::process::id());
    assert_eq!(checker_before["live_identity"]["parent"], owner["pid"]);
    assert_eq!(checker_before["live_identity"]["session"], owner["session"]);
    assert_eq!(checker_before["live_identity"]["group"], owner["group"]);
    let unarmed_peer = pidfd_open(Pid::from_raw(pid).unwrap(), PidfdFlags::empty()).unwrap();
    let peer_before = observe_process(
        pid as u32,
        &unarmed_peer,
        &inputs.binary,
        &inputs.binary_sha256,
    );
    assert_eq!(peer_before["live_identity"]["parent"], checker_pid);
    assert_eq!(peer_before["live_identity"]["group"], pid);
    assert_eq!(
        peer_before["live_identity"]["session"],
        checker_before["live_identity"]["session"]
    );
    assert!(
        peer_before["live_identity"]["start_ticks"]
            .as_u64()
            .unwrap()
            >= checker_before["live_identity"]["start_ticks"]
                .as_u64()
                .unwrap()
    );
    let expected_output = output.join(if mode == "first" {
        "console-0/rust"
    } else {
        "transport/runtime-prefix"
    });
    assert_eq!(Path::new(held["output"].as_str().unwrap()), expected_output);
    let invocation: Value =
        serde_json::from_slice(&fs::read(expected_output.join("invocation.json")).unwrap())
            .unwrap();
    assert_eq!(invocation["pid"], held["pid"]);
    assert_eq!(invocation["original"], false);
    assert_eq!(invocation["binary_sha256"], inputs.binary_sha256);
    assert_eq!(invocation["source_commit"], inputs.commit);
    assert!(!output.join("console-0/source").exists());
    assert!(!output.join("transport/source").exists());
    // A descriptor executable path is expected in this CLI. Bind its live inode
    // to the fresh provider before signalling rather than reopening that path later.
    let fd = invocation["binary"]
        .as_str()
        .unwrap()
        .strip_prefix("/proc/self/fd/")
        .unwrap();
    assert!(fd.bytes().all(|b| b.is_ascii_digit()));
    let executable = fs::metadata(format!(
        "/proc/{}/fd/{fd}",
        checker.child.as_ref().unwrap().id()
    ))
    .unwrap();
    let fresh = fs::metadata(&inputs.binary).unwrap();
    assert_eq!(
        (executable.dev(), executable.ino()),
        (fresh.dev(), fresh.ino())
    );
    let peer_exe = fs::metadata(format!("/proc/{pid}/exe")).unwrap();
    assert_eq!((peer_exe.dev(), peer_exe.ino()), (fresh.dev(), fresh.ino()));
    // All invocation/scope/FD/provider and family proofs precede this sole arm.
    assert_eq!(
        checker_before,
        observe_process(
            checker_pid,
            &checker.identity,
            Path::new(CHECKER),
            &checker_sha256
        )
    );
    assert_eq!(
        peer_before,
        observe_process(
            pid as u32,
            &unarmed_peer,
            &inputs.binary,
            &inputs.binary_sha256
        )
    );
    held["checker_observation"] = checker_before;
    held["peer_live_identity"] = peer_before["live_identity"].clone();
    held["peer_observation"] = peer_before;
    checker.peer = Some(unarmed_peer);
    (root, output, checker, held, actual_argv)
}
struct Descendants {
    identities: Vec<(Value, OwnedFd)>,
}
impl Descendants {
    fn capture(
        checker: &mut Checker,
        root: &Path,
        held: &Value,
        provider: &Path,
        provider_sha256: &str,
    ) -> Self {
        let directory = root.join("records/escaped");
        checker.wait(
            Duration::from_secs(15),
            "actual session-escaped native children",
            || directory.join("escaped.json").is_file() && directory.join("leaf.json").is_file(),
        );
        let checker_pid = checker.child.as_ref().unwrap().id();
        let producer_pid: u32 = held["pid"].as_u64().unwrap().try_into().unwrap();
        let checker_sha256 = held["checker_observation"]["sha256"].as_str().unwrap();
        assert_eq!(
            held["checker_observation"],
            observe_process(
                checker_pid,
                &checker.identity,
                Path::new(CHECKER),
                checker_sha256
            )
        );
        assert_eq!(
            held["peer_observation"],
            observe_process(
                producer_pid,
                checker.peer.as_ref().unwrap(),
                provider,
                provider_sha256
            )
        );
        // No Drop-enabled Descendants exists until BOTH rows and the whole
        // family chain pass. Unwinding this vector only closes unarmed FDs.
        let mut unarmed = Vec::new();
        for role in ["escaped", "leaf"] {
            let mut row: Value =
                serde_json::from_slice(&fs::read(directory.join(format!("{role}.json"))).unwrap())
                    .unwrap();
            assert_eq!(row["role"], role);
            let pid = Pid::from_raw(row["pid"].as_i64().unwrap().try_into().unwrap()).unwrap();
            let fd = pidfd_open(pid, PidfdFlags::empty()).unwrap();
            let observation =
                observe_process(pid.as_raw_pid() as u32, &fd, provider, provider_sha256);
            for key in ["pid", "parent", "group", "session"] {
                assert_eq!(row[key], observation["live_identity"][key]);
            }
            row["live_identity"] = observation["live_identity"].clone();
            row["observation"] = observation;
            unarmed.push((row, fd));
        }
        let escaped = &unarmed[0].0;
        let leaf = &unarmed[1].0;
        assert_eq!(
            held["peer_observation"]["live_identity"]["parent"],
            checker_pid
        );
        assert_eq!(escaped["parent"], producer_pid);
        assert_eq!(escaped["pid"], escaped["session"]);
        assert_eq!(escaped["pid"], escaped["group"]);
        assert_ne!(
            escaped["session"],
            held["peer_observation"]["live_identity"]["session"]
        );
        assert_ne!(
            escaped["group"],
            held["peer_observation"]["live_identity"]["group"]
        );
        assert_eq!(leaf["parent"], escaped["pid"]);
        assert_eq!(leaf["session"], escaped["session"]);
        assert_eq!(leaf["group"], escaped["group"]);
        assert!(
            escaped["live_identity"]["start_ticks"].as_u64().unwrap()
                >= held["peer_live_identity"]["start_ticks"].as_u64().unwrap()
        );
        assert!(
            leaf["live_identity"]["start_ticks"].as_u64().unwrap()
                >= escaped["live_identity"]["start_ticks"].as_u64().unwrap()
        );
        // Recheck live PID/UID/start/FD/executable bindings before transferring
        // either quarantine descriptor into the emergency signalling guard.
        for (row, fd) in &unarmed {
            let pid: u32 = row["pid"].as_u64().unwrap().try_into().unwrap();
            assert_eq!(
                row["observation"],
                observe_process(pid, fd, provider, provider_sha256)
            );
        }
        assert_eq!(
            held["checker_observation"],
            observe_process(
                checker_pid,
                &checker.identity,
                Path::new(CHECKER),
                checker_sha256
            )
        );
        assert_eq!(
            held["peer_observation"],
            observe_process(
                producer_pid,
                checker.peer.as_ref().unwrap(),
                provider,
                provider_sha256
            )
        );
        Self {
            identities: unarmed,
        }
    }
    fn assert_reaped(&self) {
        for (row, fd) in &self.identities {
            assert_ne!(
                rustix::event::poll(
                    &mut [rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN)],
                    Some(&rustix::event::Timespec {
                        tv_sec: 0,
                        tv_nsec: 0
                    })
                )
                .unwrap(),
                0
            );
            assert!(
                !Path::new(&format!("/proc/{}", row["pid"].as_i64().unwrap())).exists(),
                "escaped child still exists, including zombie"
            );
        }
    }
    fn records(&self) -> Vec<Value> {
        self.identities.iter().map(|(row, _)| row.clone()).collect()
    }
}
impl Drop for Descendants {
    fn drop(&mut self) {
        // Failure cleanup only. Success assertions above run before this guard.
        for (_, fd) in &self.identities {
            let _ = pidfd_send_signal(fd, Signal::KILL);
        }
    }
}
fn lifecycle_case(
    key: &str,
    mode: &str,
    signal: Option<Signal>,
    foreign_directory: bool,
    escaped: bool,
) {
    let inputs = Inputs::actions();
    let (root, output, mut checker, held, actual_argv) = begin(&inputs, mode, escaped);
    let descendants = escaped.then(|| {
        Descendants::capture(
            &mut checker,
            &root,
            &held,
            &inputs.binary,
            &inputs.binary_sha256,
        )
    });
    let (path, private) = paths(mode, &output, &held);
    let foreign = foreign_directory.then(|| Foreign::new(&path, "directory"));
    if foreign_directory {
        fs::write(path.join("sentinel"), b"retained foreign directory bytes").unwrap();
    }
    let before = foreign_directory.then(|| node_snapshot(&path));
    let shutdown = Instant::now();
    let expected = if let Some(signal) = signal {
        pidfd_send_signal(checker.peer.as_ref().unwrap(), Signal::STOP).unwrap();
        pidfd_send_signal(&checker.identity, signal).unwrap();
        128 + signal.as_raw()
    } else {
        pidfd_send_signal(checker.peer.as_ref().unwrap(), Signal::KILL).unwrap();
        fs::write(
            root.join("records/release"),
            b"release after actual native producer kill",
        )
        .unwrap();
        1
    };
    assert_eq!(checker.finish().code(), Some(expected));
    assert!(shutdown.elapsed() < Duration::from_secs(20));
    assert!(!Path::new(&format!("/proc/{}", held["pid"].as_u64().unwrap())).exists());
    assert!(
        !private.exists(),
        "owned IPC directory retained after cleanup"
    );
    if let Some(foreign) = &foreign {
        foreign.assert_preserved("directory");
        assert_eq!(before.as_ref().unwrap(), &node_snapshot(&path));
    } else {
        assert!(!path.exists());
    }
    if signal.is_none() {
        let stderr = fs::read_to_string(&checker.stderr).unwrap();
        assert!(
            stderr.contains("closed stderr")
                || stderr.contains("Broken pipe")
                || stderr.contains("broken pipe"),
            "{stderr}"
        );
    }
    if let Some(descendants) = &descendants {
        descendants.assert_reaped();
    }
    assert!(
        !output.join("report.json").exists(),
        "interrupted attempt left a success receipt"
    );
    inputs.unchanged();
    save(
        &root,
        "retained-case.json",
        json!({
            "case_key":key,"cli_mode":"retained","hold":mode,"foreign_directory":foreign_directory,
            "returncode":expected,"controller_argv":actual_argv,"held":held,"actual_producer_reaped":true,
            "owned_directory_removed":true,"foreign_before":before,"foreign_after":foreign_directory.then(|| node_snapshot(&path)),
            "escaped_descendants":descendants.as_ref().map(Descendants::records).unwrap_or_default(),
            "escaped_descendants_reaped":escaped,"shutdown_seconds":shutdown.elapsed().as_secs_f64(),
            "native_sha256":inputs.binary_sha256,"context_index_sha256":inputs.context_sha256,"source_commit":inputs.commit,
            "R4_acceptance":false,"lifecycle_reuse_granted":false,"requires_external_full_descendant_trace":true
        }),
    );
}

#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn first_int() {
    lifecycle_case(
        "retained-first-int",
        "first",
        Some(Signal::INT),
        false,
        false,
    );
}
#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn first_term() {
    lifecycle_case(
        "retained-first-term",
        "first",
        Some(Signal::TERM),
        false,
        false,
    );
}
#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn first_eof() {
    lifecycle_case("retained-first-eof", "first", None, false, false);
}
#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn runtime_directory_int() {
    lifecycle_case(
        "retained-runtime-directory-int",
        "runtime",
        Some(Signal::INT),
        true,
        false,
    );
}
#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn runtime_directory_term() {
    lifecycle_case(
        "retained-runtime-directory-term",
        "runtime",
        Some(Signal::TERM),
        true,
        false,
    );
}
#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn escaped_descendant_int() {
    lifecycle_case(
        "retained-escaped-descendant-int",
        "first",
        Some(Signal::INT),
        false,
        true,
    );
}
#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn escaped_descendant_term() {
    lifecycle_case(
        "retained-escaped-descendant-term",
        "first",
        Some(Signal::TERM),
        false,
        true,
    );
}
#[test]
#[ignore = "Actions-only fresh retained checker/native/context; shared600s parent supervisor"]
fn escaped_descendant_eof() {
    lifecycle_case(
        "retained-escaped-descendant-eof",
        "first",
        None,
        false,
        true,
    );
}

fn collision(kind: &str) {
    let inputs = Inputs::actions();
    let root = root();
    let output = root.join("output");
    let external = root.join("external");
    fs::create_dir(&external).unwrap();
    fs::write(
        external.join("sentinel"),
        b"retained external foreign bytes",
    )
    .unwrap();
    match kind {
        "file" => fs::write(&output, b"retained foreign output file").unwrap(),
        "directory" | "live-directory" => {
            fs::create_dir(&output).unwrap();
            fs::write(
                output.join("sentinel"),
                b"retained foreign output directory",
            )
            .unwrap();
            fs::write(output.join("report.json"), b"foreign report").unwrap();
            fs::write(
                output.join("providers.after.json"),
                b"foreign provider receipt",
            )
            .unwrap();
            symlink(
                external.join("sentinel"),
                output.join("providers.before.json"),
            )
            .unwrap();
        }
        "symlink" => symlink(&external, &output).unwrap(),
        "dangling-symlink" => symlink(root.join("missing"), &output).unwrap(),
        _ => panic!("unknown retained collision kind"),
    }
    let _live = (kind == "live-directory").then(|| UnixListener::bind(output.join("s")).unwrap());
    let before = json!({"output":node_snapshot(&output),"external":node_snapshot(&external)});
    let stderr = root.join("checker.stderr");
    let mut command = inputs.command(&output);
    let actual_argv = argv(&command);
    let child = command
        .env("NATIVE_LOGGING_QA_HOLD", "first")
        .env("NATIVE_LOGGING_QA_RECORDS", root.join("unexpected-child"))
        .stdout(File::create(root.join("checker.stdout")).unwrap())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut checker = guarded(child, stderr);
    assert_eq!(checker.finish().code(), Some(1));
    assert!(fs::read_to_string(&checker.stderr)
        .unwrap()
        .contains("logging evidence output already exists"));
    let after = json!({"output":node_snapshot(&output),"external":node_snapshot(&external)});
    assert_eq!(before, after);
    assert!(!root.join("unexpected-child").exists());
    assert!(fs::read(root.join("checker.stdout")).unwrap().is_empty());
    inputs.unchanged();
    save(
        &root,
        "retained-collision.json",
        json!({"case_key":format!("retained-output-{kind}"),"cli_mode":"retained", "kind":kind,
        "returncode":1,"before":before,"after":after,"producer_barrier_absent":true,"controller_argv":actual_argv,
        "zero_child_exec_requires_external_trace":true,"R4_acceptance":false,"lifecycle_reuse_granted":false}),
    );
}
#[test]
#[ignore = "Actions-only actual retained CLI output collision"]
fn output_file() {
    collision("file");
}
#[test]
#[ignore = "Actions-only actual retained CLI output collision"]
fn output_directory() {
    collision("directory");
}
#[test]
#[ignore = "Actions-only actual retained CLI output collision"]
fn output_live_directory() {
    collision("live-directory");
}
#[test]
#[ignore = "Actions-only actual retained CLI output collision"]
fn output_symlink() {
    collision("symlink");
}
#[test]
#[ignore = "Actions-only actual retained CLI output collision"]
fn output_dangling_symlink() {
    collision("dangling-symlink");
}

#[test]
#[ignore = "Actions-only actual retained CLI rejects executable hooks before child work"]
fn forbidden_executable_options() {
    let inputs = Inputs::actions();
    for option in ["--builder", "--reference", "--source-hook"] {
        let root = root();
        let output = root.join("output");
        let stderr = root.join("checker.stderr");
        let mut command = inputs.command(&output);
        command.arg(option).arg(&inputs.binary);
        let actual_argv = argv(&command);
        let child = command
            .stdout(File::create(root.join("checker.stdout")).unwrap())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let mut checker = guarded(child, stderr);
        assert_eq!(checker.finish().code(), Some(2));
        let error = fs::read_to_string(&checker.stderr).unwrap();
        assert!(error.contains(option), "{error}");
        assert!(!output.exists());
        inputs.unchanged();
        save(
            &root,
            "retained-control.json",
            json!({"case_key":format!("retained-reject-{}", &option[2..]),
            "cli_mode":"retained","returncode":2,"controller_argv":actual_argv,
            "zero_child_exec_requires_external_trace":true,"R4_acceptance":false,"lifecycle_reuse_granted":false}),
        );
    }
}
