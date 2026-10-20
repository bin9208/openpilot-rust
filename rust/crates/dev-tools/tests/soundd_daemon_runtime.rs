//! Mandatory real IPC/daemon checks. No success peer, optional skip, or physical audio.
#![cfg(feature = "soundd-daemon-native")]
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    soundd_daemon_check::{self as check, Event, Mode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn workspace() -> PathBuf {
    let base = std::env::var_os("SOUNDD_DAEMON_TEST_OUTPUT")
        .expect("mandatory artifact directory missing: SOUNDD_DAEMON_TEST_OUTPUT");
    fs::create_dir_all(&base).unwrap();
    // Keep raw events, samples, stderr and namespace witnesses on assertion
    // failure; Actions uploads this small audio-only QA artifact directory.
    tempfile::Builder::new()
        .prefix("case-")
        .tempdir_in(base)
        .unwrap()
        .keep()
}
fn prerequisite(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("mandatory soundd CI prerequisite missing: {name}")),
    );
    assert!(
        path.is_file(),
        "{name} is not a regular artifact: {}",
        path.display()
    );
    fs::canonicalize(path).unwrap()
}
struct Tools {
    worker: PathBuf,
    daemon: PathBuf,
    assets: PathBuf,
    library: PathBuf,
}
fn tools() -> Tools {
    Tools {
        worker: prerequisite("SOUNDD_DAEMON_WORKER_BINARY"),
        daemon: prerequisite("SOUNDD_DAEMON_BINARY"),
        assets: prerequisite("SOUNDD_FIXTURE_ASSETS_BINARY"),
        library: prerequisite("SOUNDD_PORTAUDIO_LIBRARY"),
    }
}
fn spec(tools: &Tools, output: &Path) -> CommandSpec {
    let mut spec = CommandSpec::new_os(
        vec![
            tools.worker.clone().into(),
            "--binary".into(),
            tools.daemon.clone().into(),
            "--output".into(),
            output.into(),
            "--fixture-assets-binary".into(),
            tools.assets.clone().into(),
            "--portaudio-library".into(),
            tools.library.clone().into(),
        ],
        Duration::from_secs(150),
    );
    spec.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        root().to_str().unwrap().into(),
    );
    spec
}
fn rows(output: &Path) -> Vec<Event> {
    fs::read_to_string(output.join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn namespace_gone(case: &Path) {
    let witness = case.join("namespace.json");
    if witness.exists() {
        let witness = value(&witness);
        let prefix = witness["prefix"].as_str().unwrap();
        assert!(prefix.starts_with("sound-"));
        assert!(
            !Path::new("/dev/shm")
                .join(format!("msgq_{prefix}"))
                .exists(),
            "owned namespace leaked"
        );
        use std::os::unix::ffi::OsStringExt;
        let temporary = PathBuf::from(OsString::from_vec(
            witness["temporary_path_bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_u64().unwrap() as u8)
                .collect(),
        ));
        assert!(!temporary.exists(), "owned fixture/Params root leaked");
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[test]
fn all_seven_real_daemon_modes_retain_every_source_predicate_and_artifact() {
    let tools = tools();
    let temp = workspace();
    let output = temp.join("all-seven");
    let observed = NativeRunner.run(&spec(&tools, &output)).unwrap();
    assert_eq!(
        observed.code,
        0,
        "{}",
        String::from_utf8_lossy(&observed.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&observed.stdout).unwrap(),
        json!({"passed":true,"scenarios":7})
    );
    let summary = value(&output.join("summary.json"));
    let summary = summary.as_array().unwrap();
    assert_eq!(summary.len(), 7);
    for (mode, summary) in Mode::ALL.into_iter().zip(summary) {
        let case = output.join(mode.as_str());
        assert_eq!(value(&case.join("summary.json")), *summary);
        assert_eq!(summary["mode"], mode.as_str());
        assert_eq!(summary["passed"], true);
        assert_eq!(summary["audio_hardware_opened"], false);
        let metrics = check::validate_events(
            mode,
            summary["pid"].as_u64().unwrap() as u32,
            summary["exit"].as_i64().unwrap() as i32,
            &rows(&case),
        )
        .unwrap();
        assert_eq!(summary["opens"], metrics.opens);
        assert_eq!(summary["callbacks"], metrics.callbacks);
        assert_eq!(summary["median_period"], json!(metrics.median_period));
        let packets = summary["packets"].as_array().unwrap();
        assert_eq!(packets.len(), if mode == Mode::Live { 10 } else { 0 });
        for (index, packet) in packets.iter().enumerate() {
            let raw = fs::read(case.join(format!("input-{index}.capnp"))).unwrap();
            let parsed = capnp::serialize::read_message(
                &mut io::Cursor::new(&raw),
                capnp::message::ReaderOptions::new(),
            )
            .unwrap();
            let event = parsed
                .get_root::<openpilot_cereal::log_capnp::event::Reader>()
                .unwrap();
            assert!(!event.get_valid());
            assert_eq!(
                raw,
                check::input_packet(
                    packet["topic"].as_str().unwrap(),
                    &packet["values"],
                    event.get_log_mono_time()
                )
                .unwrap()
            );
            assert!(packet["time"].as_f64().unwrap() * 1e9 >= event.get_log_mono_time() as f64);
        }
        assert!(case.join("stdout.log").exists());
        assert!(case.join("stderr.log").exists());
        namespace_gone(&case);
    }
    let provenance = value(&output.join("provenance.json"));
    assert_eq!(provenance["physical_audio_tested"], false);
    for (key, path) in [
        ("daemon", &tools.daemon),
        ("fixture_assets", &tools.assets),
        ("checker_worker", &tools.worker),
        ("owned_portaudio", &tools.library),
    ] {
        assert_eq!(
            provenance["executables"][key],
            format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
        );
    }
    assert_eq!(
        provenance["source_sha256"],
        check::validate_source(&root()).unwrap()
    );
    assert_eq!(
        provenance["historical_source_sha256"]["rust/tools/check_soundd_daemon.py"],
        "fadc1ad42b382d4e4a6be128ead810e2b694e69b937525cb280e8f59cd799664"
    );
}
#[test]
fn six_original_failure_artifact_boundaries_preserve_foreign_sentinels_and_cleanup() {
    let tools = tools();
    let captures: Value = serde_json::from_slice(include_bytes!(
        "fixtures/soundd-daemon/failure-original.json"
    ))
    .unwrap();
    for original in captures["cases"].as_array().unwrap() {
        let temp = workspace();
        let output = temp.join("output");
        let name = original["name"].as_str().unwrap();
        if name == "foreign-existing-output" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("summary.json"), b"foreign-summary-sentinel\n").unwrap();
        }
        if name == "output-is-file" {
            fs::write(&output, b"foreign-output-file\n").unwrap();
        }
        let mut command = spec(&tools, &output);
        let argv = command.os_argv.as_mut().unwrap();
        argv.extend([OsString::from("--mode"), "bounded".into()]);
        // Values after named options are independent process-boundary controls,
        // not invented daemon replies or substitutes for the seven real modes.
        if matches!(name, "missing-library" | "foreign-existing-output") {
            argv[8] = temp.join("absent-owned-library").into();
        }
        if name == "asset-nonzero" {
            argv[6] = "/usr/bin/false".into();
        }
        if name == "missing-daemon" {
            argv[2] = temp.join("absent-daemon").into();
        }
        if name == "bounded-nonzero-daemon" {
            argv[2] = "/usr/bin/false".into();
        }
        let observed = NativeRunner.run(&command).unwrap();
        assert_ne!(observed.code, 0, "{name}");
        for (path, expected) in original["artifact_presence"].as_object().unwrap() {
            assert_eq!(
                output.is_dir() && output.join(path).exists(),
                expected.as_bool().unwrap(),
                "{name}: {path}"
            );
        }
        assert_eq!(
            output.exists(),
            original["output_exists"].as_bool().unwrap()
        );
        assert_eq!(
            output.is_file(),
            original["output_is_file"].as_bool().unwrap()
        );
        if let Some(expected) = original["preserved_sentinel_hex"].as_str() {
            let bytes = fs::read(if output.is_file() {
                output.clone()
            } else {
                output.join("summary.json")
            })
            .unwrap();
            assert_eq!(hex(&bytes), expected, "{name}");
        }
        namespace_gone(&output.join("bounded"));
    }
}
#[test]
fn all_fifteen_cli_process_statuses_stdout_and_stderr_match_original() {
    let tools = tools();
    let original: Value =
        serde_json::from_slice(include_bytes!("fixtures/soundd-daemon/cli-original.json")).unwrap();
    for case in original["cases"].as_array().unwrap() {
        use std::os::unix::ffi::OsStringExt;
        let mut argv = vec![tools.worker.clone().into_os_string()];
        argv.extend(case["args_bytes"].as_array().unwrap().iter().map(|a| {
            OsString::from_vec(
                a.as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b.as_u64().unwrap() as u8)
                    .collect(),
            )
        }));
        let got = NativeRunner
            .run(&CommandSpec::new_os(argv, Duration::from_secs(10)))
            .unwrap();
        assert_eq!(
            got.code,
            case["code"].as_i64().unwrap() as i32,
            "{}",
            case["name"]
        );
        for (observed, key) in [(&got.stdout, "stdout"), (&got.stderr, "stderr")] {
            assert_eq!(
                observed.as_slice(),
                case[key]
                    .as_str()
                    .unwrap()
                    .replace("check_soundd_daemon.py", "check-soundd-daemon")
                    .as_bytes(),
                "{} {key}",
                case["name"]
            );
        }
    }
}
#[test]
fn default_compiles_only_the_owned_portaudio_abi_fixture() {
    let tools = tools();
    let temp = workspace();
    let output = temp.join("compiled-fixture");
    let mut command = spec(&tools, &output);
    let argv = command.os_argv.as_mut().unwrap();
    argv.truncate(7);
    argv.extend([OsString::from("--mode"), "bounded".into()]);
    let got = NativeRunner.run(&command).unwrap();
    assert_eq!(got.code, 0, "{}", String::from_utf8_lossy(&got.stderr));
    assert!(fs::read_to_string(output.join("compile.log"))
        .unwrap()
        .ends_with("compiled owned PortAudio ABI fixture\n"));
    assert!(output.join("libowned-portaudio.so").is_file());
    assert_eq!(
        value(&output.join("summary.json"))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    namespace_gone(&output.join("bounded"));
}

// Outer test owns this process group and retains its leader until every cleanup
// decision. Killing an unreaped leader's group cannot target a recycled PGID.
struct CheckerChild(Option<std::process::Child>);
impl CheckerChild {
    fn wait(&mut self) -> std::process::ExitStatus {
        use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
        let child = self.0.as_ref().unwrap();
        let pid = Pid::from_child(child);
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        while waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .is_none()
        {
            assert!(std::time::Instant::now() < deadline, "checker did not stop");
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        self.0.take().unwrap().wait().unwrap()
    }
}
impl Drop for CheckerChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = rustix::process::kill_process_group(
                rustix::process::Pid::from_child(&child),
                rustix::process::Signal::KILL,
            );
            let _ = child.wait();
        }
    }
}
fn spawn_checker(spec: &CommandSpec, closed_stdout: bool) -> CheckerChild {
    use std::{
        os::unix::process::CommandExt,
        process::{Command, Stdio},
    };
    let argv = spec.effective_argv();
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .envs(spec.effective_env())
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().unwrap();
    if closed_stdout {
        drop(child.stdout.take());
    }
    CheckerChild(Some(child))
}
#[test]
fn broken_stdout_cannot_publish_a_new_pass_or_remove_a_foreign_receipt() {
    let tools = tools();
    let temp = workspace();
    let output = temp.join("broken-stdout");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("summary.json"), b"foreign-summary-sentinel\n").unwrap();
    let mut command = spec(&tools, &output);
    command
        .os_argv
        .as_mut()
        .unwrap()
        .extend([OsString::from("--mode"), "bounded".into()]);
    let mut child = spawn_checker(&command, true);
    assert!(!child.wait().success());
    assert_eq!(
        fs::read(output.join("summary.json")).unwrap(),
        b"foreign-summary-sentinel\n"
    );
    namespace_gone(&output.join("bounded"));
}
#[test]
fn cancellation_reaps_owned_worker_and_daemon_before_namespace_cleanup() {
    let tools = tools();
    let temp = workspace();
    let output = temp.join("cancel");
    let mut command = spec(&tools, &output);
    command
        .os_argv
        .as_mut()
        .unwrap()
        .extend([OsString::from("--mode"), "exhausted".into()]);
    let mut child = spawn_checker(&command, false);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let path = output.join("exhausted/events.jsonl");
        if fs::read_to_string(path).is_ok_and(|s| {
            s.lines()
                .filter_map(|s| serde_json::from_str::<Event>(s).ok())
                .any(|e| e.event == "open")
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actual daemon did not open owned fixture"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    rustix::process::kill_process(
        rustix::process::Pid::from_child(child.0.as_ref().unwrap()),
        rustix::process::Signal::INT,
    )
    .unwrap();
    assert!(!child.wait().success());
    assert!(!output.join("summary.json").exists());
    namespace_gone(&output.join("exhausted"));
}

#[test]
fn caller_stdin_reaches_the_real_asset_helper_through_both_workers() {
    let tools = tools();
    let temp = workspace();
    let output = temp.join("stdin");
    let witness = temp.join("helper-stdin");
    let launcher =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/soundd-daemon/assets-stdin");
    let mut command = spec(&tools, &output);
    let argv = command.os_argv.as_mut().unwrap();
    argv[6] = launcher.into();
    argv.extend([OsString::from("--mode"), "bounded".into()]);
    let mut environment = command.effective_env();
    environment.insert("SOUNDD_REAL_ASSETS".into(), tools.assets.into());
    environment.insert("SOUNDD_HELPER_STDIN_WITNESS".into(), witness.clone().into());
    command.set_os_env(environment);
    command.input = Some(b"owned caller stdin\n".to_vec());
    let got = NativeRunner.run(&command).unwrap();
    assert_eq!(got.code, 0, "{}", String::from_utf8_lossy(&got.stderr));
    assert_eq!(fs::read(witness).unwrap(), b"owned caller stdin\n");
    assert_eq!(value(&output.join("summary.json"))[0]["passed"], true);
    let summary = value(&output.join("bounded/summary.json"));
    check::validate_events(
        Mode::Bounded,
        summary["pid"].as_u64().unwrap() as u32,
        summary["exit"].as_i64().unwrap() as i32,
        &rows(&output.join("bounded")),
    )
    .unwrap();
    namespace_gone(&output.join("bounded"));
}

#[test]
fn source_or_daemon_mutation_during_final_stdout_cannot_publish_success() {
    use rustix::{
        fs::{fcntl_getfl, fcntl_setfl, OFlags},
        process::{waitid, Pid, WaitId, WaitIdOptions},
    };
    use std::{
        io::{ErrorKind, Read, Write},
        os::unix::process::CommandExt,
        process::{Command, Stdio},
        time::Instant,
    };
    let tools = tools();
    let manifest: Value =
        serde_json::from_slice(include_bytes!("fixtures/soundd-daemon/manifest.json")).unwrap();
    for boundary in ["source", "daemon"] {
        let temp = workspace();
        let output = temp.join("blocked-stdout");
        let copied_root = temp.join("source");
        for name in manifest["source_sha256"].as_object().unwrap().keys() {
            let source = root().join(name);
            match fs::symlink_metadata(&source) {
                Err(e)
                    if e.kind() == ErrorKind::NotFound
                        && name == "rust/tools/check_soundd_daemon.py" =>
                {
                    continue
                }
                result => {
                    result.unwrap();
                }
            }
            let destination = copied_root.join(name);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source, destination).unwrap();
        }
        let daemon = temp.join("soundd-rs");
        fs::copy(&tools.daemon, &daemon).unwrap();
        let mut command = spec(&tools, &output);
        command.env.insert(
            "OPENPILOT_RUST_ROOT".into(),
            copied_root.to_str().unwrap().into(),
        );
        let argv = command.os_argv.as_mut().unwrap();
        argv[2] = daemon.clone().into();
        argv.extend([OsString::from("--mode"), "bounded".into()]);
        let (mut reader, mut writer) = std::io::pipe().unwrap();
        let flags = fcntl_getfl(&writer).unwrap();
        fcntl_setfl(&writer, flags | OFlags::NONBLOCK).unwrap();
        loop {
            match writer.write(&[b'x'; 4096]) {
                Ok(n) => assert!(n > 0),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => (),
                Err(e) => panic!("{e}"),
            }
        }
        fcntl_setfl(&writer, flags).unwrap();
        let flags = fcntl_getfl(&reader).unwrap();
        fcntl_setfl(&reader, flags | OFlags::NONBLOCK).unwrap();
        let argv = command.effective_argv();
        let child = Command::new(&argv[0])
            .args(&argv[1..])
            .envs(command.effective_env())
            .process_group(0)
            .stdout(Stdio::from(writer))
            .stderr(fs::File::create(temp.join("stderr.log")).unwrap())
            .spawn()
            .unwrap();
        let mut owner = CheckerChild(Some(child));
        let child = owner.0.as_ref().unwrap();
        let pid = Pid::from_child(child);
        let proc_dir = PathBuf::from(format!("/proc/{}", child.id()));
        let observe = |phase: &str, drained: u64, tail: &[u8]| {
            let read = |name: &str| {
                fs::read_to_string(proc_dir.join(name))
                    .unwrap_or_else(|error| format!("unavailable: {error}"))
            };
            let stdout_fd = fs::read_link(proc_dir.join("fd/1"))
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| format!("unavailable: {error}"));
            json!({
                "phase":phase,
                "drained_bytes":drained,
                "drained_tail_hex":hex(tail),
                "syscall":read("syscall"),
                "status":read("status"),
                "wchan":read("wchan"),
                "stdout_fd":stdout_fd,
                "stdout_fdinfo":read("fdinfo/1"),
                "stderr":fs::read_to_string(temp.join("stderr.log")).unwrap_or_default(),
                "root_summary_exists":output.join("summary.json").exists()
            })
        };
        let end = Instant::now() + Duration::from_secs(30);
        let mut at_final_stdout = None;
        loop {
            assert!(
                waitid(
                    WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
                )
                .unwrap()
                .is_none(),
                "{boundary}: {}",
                fs::read_to_string(temp.join("stderr.log")).unwrap()
            );
            let final_inputs_written = output.join("provenance.json").is_file()
                && output.join("bounded/summary.json").is_file()
                && !output.join("summary.json").exists();
            if final_inputs_written {
                // The pipe was filled before spawn and has no reader until the
                // mutation below. After provenance, the controller verifies
                // inputs and writes final stdout before publication. A stable
                // live child here is evidence of the full-pipe boundary, though
                // it cannot prove the preceding verification has completed.
                // /proc/<pid>/wchan reported "0" in GitHub Actions.
                if at_final_stdout.get_or_insert_with(Instant::now).elapsed()
                    >= Duration::from_millis(500)
                {
                    break;
                }
            } else {
                at_final_stdout = None;
            }
            assert!(
                Instant::now() < end,
                "{boundary}: final stdout not reached: provenance={}, bounded summary={}, root summary={}",
                output.join("provenance.json").exists(),
                output.join("bounded/summary.json").exists(),
                output.join("summary.json").exists()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!output.join("summary.json").exists());
        let mut observations = vec![observe("predicate", 0, &[])];
        // All mutations are confined to copied inputs. The real daemon and
        // repository source used by other tests/CI steps are never changed.
        if boundary == "source" {
            let path = copied_root.join("rust/tools/fixtures/soundd/portaudio.c");
            let mut bytes = fs::read(&path).unwrap();
            bytes.extend_from_slice(b"\n/* private late mutation */\n");
            fs::write(path, bytes).unwrap();
        } else {
            let changed = temp.join("replacement-daemon");
            fs::write(&changed, b"private changed executable identity\n").unwrap();
            fs::rename(changed, &daemon).unwrap();
        }
        observations.push(observe("mutated", 0, &[]));
        let end = Instant::now() + Duration::from_secs(5);
        let mut drained = 0_u64;
        let mut tail = Vec::new();
        loop {
            let mut bytes = [0; 8192];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => {
                        drained += count as u64;
                        tail.extend_from_slice(&bytes[..count]);
                        if tail.len() > 256 {
                            let excess = tail.len() - 256;
                            tail.drain(..excess);
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => (),
                    Err(e) => panic!("{e}"),
                }
            }
            if waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .unwrap()
            .is_some()
            {
                break;
            }
            if Instant::now() >= end {
                observations.push(observe("timeout", drained, &tail));
                fs::write(
                    temp.join("late-stdout-diagnostic.json"),
                    serde_json::to_vec_pretty(&observations).unwrap(),
                )
                .unwrap();
                panic!("{boundary}: checker did not reject changed input");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        observations.push(observe("exited", drained, &tail));
        fs::write(
            temp.join("late-stdout-diagnostic.json"),
            serde_json::to_vec_pretty(&observations).unwrap(),
        )
        .unwrap();
        assert!(
            !owner.wait().success(),
            "{boundary}: stale provenance accepted"
        );
        assert!(!output.join("summary.json").exists());
        namespace_gone(&output.join("bounded"));
    }
}
