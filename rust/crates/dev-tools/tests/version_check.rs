//! Native Version controllers against independent original policy and actual IPC.
#![cfg(feature = "version-tools-native")]

use openpilot_dev_tools::{
    version_collector_check as collector, version_reference_check as reference,
};
use openpilot_log_format::{JsonValue, JsonView};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Cursor,
    os::unix::{ffi::OsStringExt, fs::symlink},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/version-tools")
            .join(name),
    )
    .unwrap()
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn environment() -> BTreeMap<OsString, OsString> {
    let mut values = std::env::vars_os().collect::<BTreeMap<_, _>>();
    values.remove(&OsString::from("VERSION_EMULATED_SPAWN"));
    values
}
fn native(name: &str) -> PathBuf {
    let path = std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} must name the current source-matched native ELF"));
    let path = fs::canonicalize(path).unwrap();
    assert!(fs::read(&path).unwrap().starts_with(b"\x7fELF"));
    path
}
fn reference_options(binary: &Path, runner: &[OsString], output: &Path) -> reference::Options {
    let mut env = environment();
    env.insert("VERSION_EVIDENCE".into(), output.as_os_str().to_owned());
    let args = std::iter::once(binary.as_os_str().to_owned())
        .chain(runner.iter().cloned())
        .collect::<Vec<_>>();
    match reference::parse(&args, &root(), &env).unwrap() {
        reference::Command::Compare(options) => options,
        reference::Command::Source => panic!("a binary path selected source mode"),
    }
}

// This comparison is only for independently captured SOURCE values. It preserves
// JSON kinds, arbitrary integers and Unicode codepoints; NaN alone compares equal.
fn same_source(expected: &JsonValue, actual: &JsonValue) {
    match (expected.view(), actual.view()) {
        (JsonView::Null, JsonView::Null) => {}
        (JsonView::Bool(a), JsonView::Bool(b)) => assert_eq!(a, b),
        (JsonView::Integer(a), JsonView::Integer(b)) => assert_eq!(a, b),
        (JsonView::Float(a), JsonView::Float(b)) => {
            assert!(a == b || a.is_nan() && b.is_nan(), "{a} != {b}");
        }
        (JsonView::Text(a), JsonView::Text(b)) => assert_eq!(a, b),
        (JsonView::Array(a), JsonView::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(&b) {
                same_source(a, b);
            }
        }
        (JsonView::Object(a), JsonView::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, a) in a {
                let (_, b) = b.iter().find(|(other, _)| *other == key).unwrap();
                same_source(&a, b);
            }
        }
        _ => panic!(
            "source value changed: {} != {}",
            expected.to_json().unwrap(),
            actual.to_json().unwrap()
        ),
    }
}

#[test]
fn actual_reference_keeps_all_171_independent_calls_real_logs_and_runner_order() {
    let probe = native("VERSION_PROBE_BINARY");
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("reference");
    let argv = temp.path().join("runner-argv.bin");
    let wrapper = temp.path().join("runner.sh");
    // The runner records the actual final argv for EVERY persistent scenario,
    // then execs the real probe. It supplies no expected protocol responses.
    fs::write(
        &wrapper,
        b"out=$1\nshift\nprintf '%s\\0' \"$@\" >> \"$out\"\nexec \"$@\"\n",
    )
    .unwrap();
    let runner = [
        OsString::from("/bin/sh"),
        wrapper.as_os_str().to_owned(),
        argv.as_os_str().to_owned(),
    ];
    let before_cwd = std::env::current_dir().unwrap();
    let before_env = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let report: Value = serde_json::from_str(
        &reference::run(&root(), &reference_options(&probe, &runner, &output))
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["comparisons"], 171);
    assert_eq!(report["matching_comparisons"], 171);
    assert_eq!(report["divergences"], json!([]));
    assert_eq!(report["binary_sha256"], hash(&probe));
    let provenance: Value = serde_json::from_slice(&fixture("provenance.json")).unwrap();
    assert_eq!(report["counts"], provenance["source_schedule_counts"]);
    for name in [
        "openpilot/system/version.py",
        "openpilot/common/git.py",
        "openpilot/common/utils.py",
    ] {
        assert_eq!(
            report["source_sha256"][name],
            provenance["source_sha256"][name]
        );
    }
    let saved: Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(saved, report);
    let expected: Vec<Value> = serde_json::from_slice(&fixture("reference-original.json")).unwrap();
    let rows = fs::read_to_string(output.join("comparisons.jsonl"))
        .unwrap()
        .lines()
        .map(|line| JsonValue::parse(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 171);
    let first_path = rows[0]
        .get("request")
        .unwrap()
        .get("path")
        .unwrap()
        .to_utf8()
        .unwrap();
    let fixture_root = first_path.strip_suffix("/clean").unwrap();
    let mut scenarios = BTreeMap::<String, usize>::new();
    for (expected, row) in expected.iter().zip(rows) {
        let name = expected["scenario"].as_str().unwrap();
        assert!(row.get("scenario").unwrap().text_eq(name));
        let request = expected["request"]
            .as_str()
            .unwrap()
            .replace("@FIXTURE_ROOT@", fixture_root);
        same_source(
            &JsonValue::parse(&request).unwrap(),
            &row.get("request").unwrap(),
        );
        assert!(matches!(
            row.get("passed").unwrap().view(),
            JsonView::Bool(true)
        ));
        let original = JsonValue::parse(expected["response"].as_str().unwrap()).unwrap();
        let logs = original.get("logs").unwrap();
        same_source(&logs, &row.get("logs").unwrap());
        let JsonView::Object(fields) = original.view() else {
            panic!("original source response is not an object");
        };
        let recorded = row.get("expected").unwrap();
        let JsonView::Object(recorded_fields) = recorded.view() else {
            panic!("recorded expected response is not an object");
        };
        assert_eq!(recorded_fields.len(), fields.len() - 1);
        for (key, value) in fields {
            if key == [108, 111, 103, 115] {
                continue;
            }
            let key = key
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .unwrap();
            same_source(&value, &recorded.get(&key).unwrap());
        }
        assert!(row.get("actual").is_some());
        *scenarios.entry(name.to_owned()).or_default() += 1;
    }
    assert_eq!(scenarios.len(), 17);
    for name in scenarios.keys() {
        assert!(output.join(format!("{name}-source.stderr")).is_file());
        assert!(output.join(format!("{name}-native.stderr")).is_file());
    }
    use std::os::unix::ffi::OsStrExt;
    let calls = fs::read(argv).unwrap();
    let mut expected_argv = probe.as_os_str().as_bytes().to_vec();
    expected_argv.push(0);
    assert_eq!(calls, expected_argv.repeat(17));
    assert_eq!(std::env::current_dir().unwrap(), before_cwd);
    assert_eq!(std::env::vars_os().collect::<BTreeMap<_, _>>(), before_env);
}

#[test]
fn positional_admission_preserves_raw_paths_exact_source_mode_and_no_file_effects() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path();
    let env = environment();
    assert!(reference::parse(&[], cwd, &env).is_err());
    assert!(matches!(
        reference::parse(&["--source".into()], cwd, &env).unwrap(),
        reference::Command::Source
    ));
    assert!(matches!(
        reference::parse(&["--source".into(), "extra".into()], cwd, &env).unwrap(),
        reference::Command::Compare(_)
    ));
    for args in [
        vec![],
        vec!["one"],
        vec!["one", "two"],
        vec!["one", "two", "out", "extra"],
    ] {
        let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
        assert!(collector::parse(&args, cwd, &env).is_err(), "{args:?}");
    }
    let raw = OsString::from_vec(b"probe-\xff space".to_vec());
    let raw_runner = OsString::from_vec(b"one argument-\xfe $()".to_vec());
    let raw_output = OsString::from_vec(b"out-\xfd".to_vec());
    fs::write(cwd.join(&raw), b"resolve only, never execute in parse").unwrap();
    symlink(&raw, cwd.join("probe-link")).unwrap();
    let mut env = env;
    env.insert("VERSION_EVIDENCE".into(), raw_output.clone());
    let args = [
        "probe-link".into(),
        "/bin/runner".into(),
        raw_runner.clone(),
    ];
    let reference::Command::Compare(options) = reference::parse(&args, cwd, &env).unwrap() else {
        panic!("probe path selected source mode");
    };
    assert_eq!(options.binary, cwd.join(&raw));
    assert_eq!(
        options.runner,
        vec![OsString::from("/bin/runner"), raw_runner]
    );
    assert_eq!(options.output, cwd.join(&raw_output));
    let options = collector::parse(
        &[raw.clone(), "collector".into(), raw_output.clone()],
        cwd,
        &env,
    )
    .unwrap();
    assert_eq!(options.probe, cwd.join(raw));
    assert_eq!(options.collector, cwd.join("collector"));
    assert_eq!(options.output, cwd.join(&raw_output));
    assert!(!cwd.join(raw_output).exists(), "parsing created evidence");
    let mut env = environment();
    env.remove(&OsString::from("VERSION_EVIDENCE"));
    let reference::Command::Compare(options) =
        reference::parse(&["candidate".into()], cwd, &env).unwrap()
    else {
        panic!("candidate selected source mode");
    };
    assert_eq!(
        options.output,
        cwd.join(".omo/evidence/runtime-version-73/reference")
    );
    assert!(!cwd.join(".omo").exists());
}

#[test]
fn source_transport_preserves_13_original_eof_error_and_recovery_controls() {
    let fixture: Value = serde_json::from_slice(&fixture("protocol-original.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let controls = fixture["cases"].as_array().unwrap();
    assert_eq!(controls.len(), 13);
    for case in controls {
        let mut input = Cursor::new(case["request"].as_str().unwrap().as_bytes());
        let mut output = Vec::new();
        let result = reference::source(temp.path(), &environment(), &mut input, &mut output);
        assert_eq!(result.is_err(), case["exit"] != 0, "{}", case["name"]);
        let stdout = String::from_utf8(output).unwrap();
        let actual = stdout.lines().collect::<Vec<_>>();
        let expected = case["stdout"].as_str().unwrap().lines().collect::<Vec<_>>();
        assert_eq!(actual.len(), expected.len(), "{}", case["name"]);
        for (expected, actual) in expected.into_iter().zip(actual) {
            same_source(
                &JsonValue::parse(expected).unwrap(),
                &JsonValue::parse(actual).unwrap(),
            );
        }
    }
}

#[test]
fn genuine_response_mismatch_is_flushed_and_does_not_publish_a_success_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let wrapper = temp.path().join("wrong-probe.sh");
    fs::write(
        &wrapper,
        b"while IFS= read -r line; do printf '%s\\n' '{\"value\":\"wrong native response\"}'; done\n",
    )
    .unwrap();
    let runner = [OsString::from("/bin/sh"), wrapper.into_os_string()];
    for emulated in [false, true] {
        let output = temp.path().join(format!("emulated-{emulated}"));
        let mut options = reference_options(Path::new("/bin/true"), &runner, &output);
        if emulated {
            options
                .environment
                .insert("VERSION_EMULATED_SPAWN".into(), "1".into());
        }
        assert!(reference::run(&root(), &options).is_err());
        let rows = fs::read_to_string(output.join("comparisons.jsonl")).unwrap();
        let rows = rows
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1, "mismatch must stop before later requests");
        assert_eq!(rows[0]["scenario"], "clean");
        assert_eq!(rows[0]["request"]["op"], "metadata");
        assert_eq!(rows[0]["actual"], json!({"value":"wrong native response"}));
        assert_eq!(rows[0]["passed"], false);
        assert!(rows[0]["expected"]["value"]["properties"].is_object());
        assert!(output.join("clean-native.stderr").is_file());
        assert!(!output.join("dirty-native.stderr").exists());
        assert!(!output.join("manifest.json").exists());
    }
}

#[test]
fn explicit_emulation_mode_never_reports_unqualified_host_pass() {
    let probe = native("VERSION_PROBE_BINARY");
    let temp = tempfile::tempdir().unwrap();
    let mut options = reference_options(&probe, &[], temp.path());
    options
        .environment
        .insert("VERSION_EMULATED_SPAWN".into(), "1".into());
    let report: Value = serde_json::from_str(
        &reference::run(&root(), &options)
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(report["result"], "EMULATION_LIMITS");
    assert_eq!(report["comparisons"], 171);
    assert_eq!(report["matching_comparisons"], 171);
    assert_eq!(report["divergences"], json!([]));
}

#[test]
fn early_probe_failures_retain_original_phase_artifacts_and_no_success() {
    let temp = tempfile::tempdir().unwrap();
    for (name, binary) in [
        ("missing", temp.path().join("missing-probe")),
        ("early-zero", PathBuf::from("/bin/true")),
        ("early-nonzero", PathBuf::from("/bin/false")),
    ] {
        let output = temp.path().join(name);
        assert!(
            reference::run(&root(), &reference_options(&binary, &[], &output)).is_err(),
            "{name}"
        );
        assert!(output.join("clean-source.stderr").is_file(), "{name}");
        assert!(output.join("clean-native.stderr").is_file(), "{name}");
        assert_eq!(
            fs::read(output.join("comparisons.jsonl")).unwrap(),
            b"",
            "{name}"
        );
        assert!(!output.join("manifest.json").exists(), "{name}");
        assert!(!output.join("dirty-native.stderr").exists(), "{name}");
    }
    let output = temp.path().join("is-file");
    fs::write(&output, b"preserve existing output file\n").unwrap();
    assert!(reference::run(
        &root(),
        &reference_options(Path::new("/bin/true"), &[], &output)
    )
    .is_err());
    assert_eq!(
        fs::read(output).unwrap(),
        b"preserve existing output file\n"
    );
}

// A pidfd is captured while the fixture still owns a LIVE sleeper. Even panic
// cleanup cannot send a signal to a recycled numeric PID or another namespace.
struct SleepOwner(OwnedFd);
impl Drop for SleepOwner {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.0, Signal::KILL);
    }
}
impl SleepOwner {
    fn exited(&self, timeout: Duration) -> bool {
        let mut descriptor = [PollFd::new(&self.0, PollFlags::IN)];
        let timeout = Timespec::try_from(timeout).unwrap();
        poll(&mut descriptor, Some(&timeout)).unwrap() == 1
    }
}

#[test]
fn exited_probe_leader_cannot_leave_its_owned_descendant_running() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("descendant.pid");
    let acknowledged = temp.path().join("identity-captured");
    let script = temp.path().join("early-exit.sh");
    fs::write(
        &script,
        b"marker=$1\nack=$2\n/bin/sleep 20 >/dev/null 2>&1 &\nprintf '%s\\n' \"$!\" > \"$marker\"\ni=0\nwhile [ ! -f \"$ack\" ] && [ \"$i\" -lt 100 ]; do /bin/sleep 0.05; i=$((i+1)); done\nexit 0\n",
    )
    .unwrap();
    let observe_marker = marker.clone();
    let observe_ack = acknowledged.clone();
    let observer = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(text) = fs::read_to_string(&observe_marker) {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    if let Some(pid) = Pid::from_raw(pid) {
                        if let Ok(fd) = pidfd_open(pid, PidfdFlags::empty()) {
                            let owner = SleepOwner(fd);
                            assert!(
                                !owner.exited(Duration::ZERO),
                                "fixture descendant was not live"
                            );
                            fs::write(&observe_ack, b"identity captured before leader exit\n")
                                .unwrap();
                            return Some(owner);
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        None
    });
    let runner = [
        OsString::from("/bin/sh"),
        script.into_os_string(),
        marker.into_os_string(),
        acknowledged.into_os_string(),
    ];
    let result = reference::run(
        &root(),
        &reference_options(Path::new("/bin/true"), &runner, &temp.path().join("output")),
    );
    let descendant = observer
        .join()
        .unwrap()
        .expect("controller did not reach actual probe startup");
    assert!(
        result.is_err(),
        "an empty early probe response was accepted"
    );
    assert!(
        descendant.exited(Duration::from_secs(1)),
        "probe descendant survived cleanup after its leader exited"
    );
}

#[test]
fn actual_collector_requires_both_real_services_and_one_exact_disk_record() {
    let probe = native("VERSION_PROBE_BINARY");
    let daemon = native("VERSION_COLLECTOR_BINARY");
    let worker = native("VERSION_WORKER_BINARY");
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("collector-proof");
    let options = collector::parse(
        &[
            probe.as_os_str().to_owned(),
            daemon.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ],
        &root(),
        &environment(),
    )
    .unwrap();
    let report = collector::run(&root(), &options, &worker).unwrap();
    assert_eq!(report["result"], "PASS");
    for name in ["logMessage", "errorLogMessage", "disk_records"] {
        assert_eq!(report[name], 1, "{name}");
    }
    assert_eq!(report["probe_sha256"], hash(&probe));
    assert_eq!(report["collector_sha256"], hash(&daemon));
    let source: Value = serde_json::from_slice(&fixture("collector-source-original.json")).unwrap();
    assert_eq!(report["source_logs"], source["logs"]);
    let source_file: Value =
        serde_json::from_slice(&fs::read(output.join("source.json")).unwrap()).unwrap();
    assert_eq!(source_file, source);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(output.join("probe.stdout")).unwrap()).unwrap(),
        json!({"error":"Exception"})
    );
    assert!(output.join("probe.stderr").is_file());
    let mut observed = Vec::new();
    for name in ["logMessage", "errorLogMessage"] {
        let raw = fs::read(output.join(format!("{name}.bin"))).unwrap();
        let message = capnp::serialize::read_message(
            &mut Cursor::new(raw),
            capnp::message::ReaderOptions::new(),
        )
        .unwrap();
        let event = message
            .get_root::<openpilot_cereal::log_capnp::event::Reader>()
            .unwrap();
        assert!(event.get_valid());
        assert!(event.get_log_mono_time() > 0);
        let text = match (name, event.which().unwrap()) {
            ("logMessage", openpilot_cereal::log_capnp::event::LogMessage(text))
            | ("errorLogMessage", openpilot_cereal::log_capnp::event::ErrorLogMessage(text)) => {
                text.unwrap()
            }
            _ => panic!("wrong collector service for {name}"),
        };
        let record: Value = serde_json::from_str(text.to_str().unwrap()).unwrap();
        for key in ["msg", "levelnum", "exc_info"] {
            assert_eq!(record[key], source["logs"][0][key], "{name}.{key}");
        }
        assert_eq!(record["ctx"]["runtime_language"], "rust");
        observed.push(record);
    }
    assert_eq!(observed[0], observed[1]);
    let records: Vec<Value> =
        serde_json::from_slice(&fs::read(output.join("records.json")).unwrap()).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["msg$s"], source["logs"][0]["msg"]);
    assert_eq!(records[0]["levelnum"], 40);
    assert_eq!(records[0]["exc_info"], source["logs"][0]["exc_info"]);
    let invocation: Value =
        serde_json::from_slice(&fs::read(output.join("collector/invocation.json")).unwrap())
            .unwrap();
    let prefix = invocation["OPENPILOT_PREFIX"].as_str().unwrap();
    assert!(!prefix.is_empty());
    assert!(!Path::new(&format!("/dev/shm/msgq_{prefix}")).exists());
    assert!(!Path::new(&format!("/tmp/logmessage{prefix}")).exists());
    let exit: Value =
        serde_json::from_slice(&fs::read(output.join("collector/exit.json")).unwrap()).unwrap();
    assert_eq!(exit["code"], 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(output.join("manifest.json")).unwrap()).unwrap(),
        report
    );
}

// Signal only the controller leader. Broadcasting to a group here would hide a
// controller that never forwards interruption into the independent source Git.
#[test]
fn actual_reference_cli_interrupts_owned_source_git_without_group_broadcast() {
    use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
    use std::{
        os::unix::{ffi::OsStrExt, fs::PermissionsExt, process::CommandExt},
        process::{Child, Command, Stdio},
    };
    struct Checker {
        child: Child,
        identity: OwnedFd,
    }
    struct MarkedGitCleanup {
        state: PathBuf,
        token: Vec<u8>,
    }
    impl Drop for MarkedGitCleanup {
        fn drop(&mut self) {
            // Failure cleanup also covers a startup assertion before the test
            // reaches its acknowledged pair. A stale marker alone grants no
            // authority: bind a live pidfd and this unique invocation's token.
            let Ok(entries) = fs::read_dir(&self.state) else {
                return;
            };
            for entry in entries.flatten() {
                if !entry.file_name().as_bytes().starts_with(b"call.") {
                    continue;
                }
                let Ok(text) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                for pid in text
                    .split_whitespace()
                    .filter_map(|text| text.parse::<i32>().ok())
                {
                    let Some(pid_identity) = Pid::from_raw(pid) else {
                        continue;
                    };
                    let Ok(identity) = pidfd_open(pid_identity, PidfdFlags::empty()) else {
                        continue;
                    };
                    let environment = fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
                    if environment
                        .split(|byte| *byte == 0)
                        .any(|entry| entry == self.token)
                    {
                        let _ = pidfd_send_signal(&identity, Signal::KILL);
                    }
                }
            }
        }
    }
    impl Drop for Checker {
        fn drop(&mut self) {
            // Only this live pidfd, never a numeric PID/PGID from a stale file.
            let _ = pidfd_send_signal(&self.identity, Signal::KILL);
            let _ = self.child.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let commands = temp.path().join("commands");
    let output = temp.path().join("evidence");
    fs::create_dir(&state).unwrap();
    fs::create_dir(&commands).unwrap();
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file() && path.metadata().unwrap().permissions().mode() & 0o111 != 0)
        .map(|path| path.canonicalize().unwrap())
        .expect("actual Git is a mandatory Version prerequisite");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/version-tools");
    symlink(fixtures.join("slow-source-git"), commands.join("git")).unwrap();
    let stdout = fs::File::create(temp.path().join("checker.stdout")).unwrap();
    let stderr = fs::File::create(temp.path().join("checker.stderr")).unwrap();
    let mut token = b"VERSION_SLOW_GIT_STATE=".to_vec();
    token.extend_from_slice(state.as_os_str().as_bytes());
    let _fallback = MarkedGitCleanup {
        state: state.clone(),
        token: token.clone(),
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_check-version-reference"))
        // The source metadata request is interrupted before any candidate call.
        // An exited /bin/true cannot leave a separate interactive probe alive
        // if an assertion fails and the fallback owner must kill the checker.
        .arg("/bin/true")
        .current_dir(root())
        .envs(environment())
        .env("PATH", &commands)
        .env("VERSION_EVIDENCE", &output)
        .env("VERSION_SLOW_GIT_STATE", &state)
        .env("VERSION_SLOW_GIT_MODE", "cancel")
        .env("VERSION_SLOW_GIT_REAL", real_git)
        .env("VERSION_SLOW_GIT_FIXTURE", fixtures.join("slow-git"))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .process_group(0)
        .spawn()
        .unwrap();
    let identity = match pidfd_open(Pid::from_child(&child), PidfdFlags::empty()) {
        Ok(identity) => identity,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("could not reserve checker identity: {error}");
        }
    };
    let mut checker = Checker { child, identity };
    let startup = Instant::now() + Duration::from_secs(10);
    let marker = state.join("call.1");
    let processes = loop {
        assert!(
            Instant::now() < startup,
            "source Git never reached acknowledged startup; checker_status={:?}; state={:?}; marker={:?}; output={:?}; children={:?}; wchan={:?}; stderr={:?}; source_stderr={:?}",
            checker.child.try_wait(),
            fs::read_dir(&state).map(|entries| entries.flatten().map(|entry| entry.file_name()).collect::<Vec<_>>()),
            fs::read_to_string(&marker),
            fs::read_dir(&output).map(|entries| entries.flatten().map(|entry| entry.file_name()).collect::<Vec<_>>()),
            fs::read_to_string(format!("/proc/{}/task/{}/children", checker.child.id(), checker.child.id())),
            fs::read_to_string(format!("/proc/{}/wchan", checker.child.id())),
            fs::read(temp.path().join("checker.stderr")),
            fs::read(output.join("clean-source.stderr")),
        );
        if let Ok(text) = fs::read_to_string(&marker) {
            if let Ok(pids) = text
                .split_whitespace()
                .map(str::parse::<i32>)
                .collect::<Result<Vec<_>, _>>()
            {
                if pids.len() == 2 {
                    let mut owners = Vec::new();
                    for pid in pids {
                        let Some(pid_identity) = Pid::from_raw(pid) else {
                            break;
                        };
                        let Ok(identity) = pidfd_open(pid_identity, PidfdFlags::empty()) else {
                            break;
                        };
                        let environment =
                            fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
                        if !environment
                            .split(|byte| *byte == 0)
                            .any(|entry| entry == token)
                        {
                            break;
                        }
                        let owner = SleepOwner(identity);
                        assert!(
                            !owner.exited(Duration::ZERO),
                            "source Git identity already exited"
                        );
                        owners.push(owner);
                    }
                    if owners.len() == 2 {
                        break owners;
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(5));
    };
    assert!(output.join("clean-source.stderr").is_file());
    assert!(
        output.join("clean-native.stderr").is_file(),
        "selector blocked fixture setup rather than source Pair"
    );
    assert!(fs::read(output.join("comparisons.jsonl"))
        .unwrap()
        .is_empty());
    fs::write(
        state.join("ack.1"),
        b"both live identities captured before SIGINT\n",
    )
    .unwrap();
    for process in &processes {
        assert!(!process.exited(Duration::ZERO));
    }
    let interrupted = Instant::now();
    pidfd_send_signal(&checker.identity, Signal::INT).unwrap();
    let deadline = interrupted + Duration::from_secs(2);
    loop {
        let mut leader = [PollFd::new(&checker.identity, PollFlags::IN)];
        let done = poll(&mut leader, Some(&Timespec::default())).unwrap() == 1;
        if done
            && processes
                .iter()
                .all(|process| process.exited(Duration::ZERO))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "SIGINT to checker leader did not promptly clean up source Git and its sleeper"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(interrupted.elapsed() < Duration::from_secs(2));
    let status = checker.child.wait().unwrap();
    assert!(!status.success(), "interrupted checker published success");
    assert!(!output.join("manifest.json").exists());
    assert!(
        fs::read(output.join("comparisons.jsonl"))
            .unwrap()
            .is_empty(),
        "source cancellation was serialized as a normal response"
    );
    assert!(
        !output.join("dirty-native.stderr").exists(),
        "controller continued to another scenario"
    );
    assert_eq!(fs::read_to_string(state.join("count")).unwrap(), "1\n");
}

fn wire_original() -> JsonValue {
    let bytes = fixture("controller-wire-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "fac9a5bec9979e1f3acec5b8da9d289f799c1c6fe2cd81582ae64951fc5a98d8"
    );
    JsonValue::parse(std::str::from_utf8(&bytes).unwrap()).unwrap()
}
#[test]
fn source_transport_preserves_original_raw_byte_strings_and_json_error_class() {
    let original = wire_original();
    let temp = tempfile::tempdir().unwrap();
    let JsonView::Array(cases) = original.get("cases").unwrap().view() else {
        panic!("captured cases must be an array")
    };
    assert_eq!(cases.len(), 2);
    for case in cases {
        let bytes = case
            .get("stdin_hex")
            .unwrap()
            .to_utf8()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        reference::source(
            temp.path(),
            &environment(),
            &mut Cursor::new(bytes),
            &mut output,
        )
        .unwrap();
        same_source(
            &JsonValue::parse(&case.get("stdout").unwrap().to_utf8().unwrap()).unwrap(),
            &JsonValue::parse(std::str::from_utf8(&output).unwrap()).unwrap(),
        );
    }
}
#[test]
fn raw_runner_manifest_keeps_original_strings_in_stdout_and_saved_report() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    let original = wire_original();
    let probe = native("VERSION_PROBE_BINARY");
    let temp = tempfile::tempdir().unwrap();
    let runner = temp
        .path()
        .join(OsString::from_vec(b"runner-\xfe.body".to_vec()));
    // /bin/sh reads this as data; only the immutable real probe is execve'd.
    fs::write(&runner, b"exec \"$@\"\n").unwrap();
    let output = temp.path().join("evidence");
    let mut env = environment();
    env.insert("OPENPILOT_RUST_ROOT".into(), root().into_os_string());
    env.insert("VERSION_EVIDENCE".into(), output.as_os_str().to_owned());
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_check-version-reference").into(),
            probe.as_os_str().to_owned(),
            "/bin/sh".into(),
            runner.into_os_string(),
        ],
        Duration::from_secs(90),
    );
    spec.set_os_env(env);
    spec.env_remove = vec!["VERSION_EMULATED_SPAWN".into()];
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        result.code,
        original
            .get("raw_runner_manifest")
            .unwrap()
            .get("exit")
            .unwrap()
            .to_json()
            .unwrap()
            .parse::<i32>()
            .unwrap(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved = fs::read(output.join("manifest.json")).unwrap();
    let stdout = JsonValue::parse(std::str::from_utf8(&result.stdout).unwrap()).unwrap();
    let saved = JsonValue::parse(std::str::from_utf8(&saved).unwrap()).unwrap();
    same_source(&saved, &stdout);
    for key in ["comparisons", "binary", "invocation"] {
        let template = original
            .get("raw_runner_manifest")
            .unwrap()
            .get(key)
            .unwrap()
            .to_json()
            .unwrap()
            .replace("@FIXTURE@", temp.path().to_str().unwrap())
            .replace("@PROBE@", probe.to_str().unwrap());
        same_source(
            &JsonValue::parse(&template).unwrap(),
            &stdout.get(key).unwrap(),
        );
    }
    let JsonView::Array(argv) = stdout.get("invocation").unwrap().view() else {
        panic!("invocation must retain an array of original filesystem strings");
    };
    assert!(argv
        .iter()
        .all(|value| matches!(value.view(), JsonView::Text(_))));
    assert!(matches!(argv[1].view(), JsonView::Text(points) if points.contains(&0xdcfe)));
}

#[test]
fn invalid_source_removes_previous_success_manifest_before_preflight() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("manifest.json"), "{\"result\":\"PASS\"}\n").unwrap();
    let options = reference::Options {
        binary: temp.path().join("missing-probe"),
        runner: vec![],
        output,
        environment: environment(),
    };
    assert!(reference::run(temp.path(), &options).is_err());
    assert!(!options.output.join("manifest.json").exists());
    assert!(!options.output.join("comparisons.jsonl").exists());
}
