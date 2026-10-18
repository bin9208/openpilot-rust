//! Independent unchanged-Python controls; no registration runtime oracle imports.
#![cfg(feature = "registration-clock-native")]
use openpilot_dev_tools::{qa_process::NativeRunner, registration_clock_check as native};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::symlink},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/registration-clock")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
fn probe() -> PathBuf {
    let path = PathBuf::from(std::env::var_os("REGISTRATION_CLOCK_BINARY").expect(
        "REGISTRATION_CLOCK_BINARY must name the fresh source-matched registration_clock ELF",
    ));
    assert!(
        path.is_absolute() && path.is_file(),
        "missing fresh registration clock prerequisite"
    );
    path
}
fn options(binary: PathBuf, output: PathBuf) -> native::Options {
    native::Options {
        root: root(),
        binary,
        output,
        runner: vec![],
        environment: std::env::vars_os().collect(),
        timeout: Duration::from_secs(60),
    }
}
fn as_value(value: &JsonValue) -> Value {
    serde_json::from_str(&value.to_json().unwrap()).unwrap()
}
#[test]
fn immutable_original_capture_hashes_and_full_independent_calendar_cases() {
    for name in ["manifest.json", "binding-manifest.json"] {
        let manifest = fixture(name);
        for (name, expected) in manifest["corpora"].as_object().unwrap() {
            let bytes = fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/registration-clock")
                    .join(name),
            )
            .unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(bytes)),
                expected.as_str().unwrap()
            );
        }
    }
    let original = fixture("policy-original.json");
    assert_eq!(native::cases().len(), 13);
    assert_eq!(
        serde_json::to_value(native::cases()).unwrap(),
        original["cases"]
    );
    for (inputs, expected) in [("cases", "source"), ("generic_cases", "generic_source")] {
        for (input, expected) in original[inputs]
            .as_array()
            .unwrap()
            .iter()
            .zip(original[expected].as_array().unwrap())
        {
            assert_eq!(
                serde_json::to_value(native::source_value(
                    input[0].as_i64().unwrap(),
                    input[1].as_u64().unwrap() as u32
                ))
                .unwrap(),
                *expected,
                "{input}"
            );
        }
    }
}
#[test]
fn all_original_argparse_actions_and_errors_keep_order() {
    for case in fixture("cli-original.json")["cases"].as_array().unwrap() {
        let args = case["args_bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                OsString::from_vec(
                    x.as_array()
                        .unwrap()
                        .iter()
                        .map(|b| b.as_u64().unwrap() as u8)
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        let result = native::parse(&args, Path::new("/cwd"), BTreeMap::new());
        let expected = |key: &str| {
            String::from_utf8(
                case[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect(),
            )
            .unwrap()
            .replace("check_registration_clock.py", "check-registration-clock")
        };
        if case["code"] == 0 {
            assert!(matches!(result.unwrap(), native::Command::Help));
            assert_eq!(native::HELP, expected("stdout_bytes"));
        } else {
            assert_eq!(
                result.unwrap_err().to_string() + "\n",
                expected("stderr_bytes"),
                "{}",
                case["name"]
            );
        }
    }
}
#[test]
fn raw_args_runner_abbreviation_and_double_dash_remain_lossless() {
    let args = vec![
        OsString::from_vec(b"probe-\xff".to_vec()),
        "--r=/usr/bin/env".into(),
        "--runner=--".into(),
        OsString::from_vec(b"out-\xfe".to_vec()),
    ];
    let native::Command::Run(o) = native::parse(&args, Path::new("/cwd"), BTreeMap::new()).unwrap()
    else {
        panic!("run")
    };
    assert_eq!(o.binary, Path::new("/cwd").join(&args[0]));
    assert_eq!(o.output, Path::new("/cwd").join(&args[3]));
    assert_eq!(
        o.runner,
        vec![OsString::from("/usr/bin/env"), OsString::from("--")]
    );
    assert!(matches!(
        native::parse(
            &["--".into(), "-binary".into(), "-output".into()],
            Path::new("/"),
            BTreeMap::new()
        )
        .unwrap(),
        native::Command::Run(_)
    ));
}
#[test]
fn real_probe_full_thirteen_and_runner_keep_exact_report_layout() {
    let temp = tempfile::tempdir().unwrap();
    let mut o = options(probe(), temp.path().join("report.json"));
    o.runner = vec!["/usr/bin/env".into(), "--".into()];
    let report = native::run_with(&o, &mut NativeRunner).unwrap();
    let report = as_value(&report);
    assert_eq!(report["source"], fixture("policy-original.json")["source"]);
    assert_eq!(report["native"], report["source"]);
    assert_eq!(report["equal"], true);
    assert_eq!(report["argv"][0], "/usr/bin/env");
    assert_eq!(report["argv"][1], "--");
    let text = fs::read_to_string(&o.output).unwrap();
    assert!(text.starts_with("{\n  \"argv\": ["));
    assert!(!text.ends_with('\n'));
    let fields = [
        "argv",
        "cases",
        "source",
        "native",
        "exit",
        "binary_sha256",
        "equal",
    ];
    let offsets = fields.map(|s| text.find(&format!("\n  \"{s}\":")).unwrap());
    assert!(offsets.windows(2).all(|w| w[0] < w[1]));
}
#[test]
fn real_raw_filesystem_paths_preserve_surrogateescape_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join(OsString::from_vec(b"probe-\xff".to_vec()));
    fs::copy(probe(), &binary).unwrap();
    let o = options(
        binary.clone(),
        temp.path().join(OsString::from_vec(b"out-\xfe".to_vec())),
    );
    let report = native::run_with(&o, &mut NativeRunner).unwrap();
    let JsonView::Array(argv) = report.get("argv").unwrap().view() else {
        panic!("argv")
    };
    assert_eq!(
        argv[0].to_json().unwrap(),
        openpilot_dev_tools::python_fs::json_string(binary.as_os_str()).unwrap()
    );
    assert!(fs::read_to_string(o.output).unwrap().contains("\\udcff"));
}
#[test]
fn original_real_child_failure_and_report_publication_phases() {
    let temp = tempfile::tempdir().unwrap();
    for case in fixture("failures-original.json")["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] != "numeric-equality")
    {
        let name = case["name"].as_str().unwrap();
        let output = temp.path().join(format!("{name}.json"));
        let mut o = options(
            match name {
                "missing" => temp.path().join("missing"),
                "nonzero" => "/bin/false".into(),
                "empty" => "/bin/true".into(),
                "mismatch" => "/bin/cat".into(),
                _ => probe(),
            },
            output,
        );
        match name {
            "malformed" => o.runner = vec!["/usr/bin/printf".into(), "{malformed".into()],
            "missing-parent" => o.output = temp.path().join("no-parent/report.json"),
            "directory-output" => {
                fs::create_dir(&o.output).unwrap();
            }
            _ => (),
        }
        let result = native::run_with(&o, &mut NativeRunner);
        assert!(result.is_err(), "{name}");
        assert_eq!(
            o.output.is_file(),
            case["report_present"].as_bool().unwrap(),
            "{name}"
        );
        if o.output.is_file() {
            let report: Value = serde_json::from_slice(&fs::read(&o.output).unwrap()).unwrap();
            assert_eq!(report["source"], case["source"]);
            assert_eq!(report["native"], case["native"]);
            assert_eq!(report["equal"], case["report_equal"]);
        }
        let error = result
            .err()
            .expect("original failure control returned success");
        match name {
            "nonzero" => assert_eq!(
                error.downcast_ref::<native::ChildFailure>().unwrap().code,
                1
            ),
            "missing" | "missing-parent" => assert_eq!(
                error
                    .downcast_ref::<std::io::Error>()
                    .unwrap()
                    .raw_os_error(),
                Some(2)
            ),
            "directory-output" => assert_eq!(
                error.downcast_ref::<std::io::Error>().unwrap().kind(),
                std::io::ErrorKind::IsADirectory
            ),
            "malformed" => assert!(error.is::<openpilot_log_format::JsonError>()),
            "empty" | "mismatch" => assert!(error.to_string().contains("report retained")),
            _ => unreachable!(),
        }
    }
}
#[test]
fn original_python_numeric_container_equality_accepts_bool_float_and_order() {
    let temp = tempfile::tempdir().unwrap();
    let case = fixture("failures-original.json")["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "numeric-equality")
        .unwrap()
        .clone();
    let payload = temp.path().join("payload");
    fs::write(
        &payload,
        case["native"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| serde_json::to_string(v).unwrap() + "\n")
            .collect::<String>(),
    )
    .unwrap();
    let mut o = options(payload, temp.path().join("report"));
    o.runner = vec!["/bin/cat".into()];
    let report = as_value(&native::run_with(&o, &mut NativeRunner).unwrap());
    assert_eq!(report["equal"], true);
    assert_eq!(report["native"], case["native"]);
}
#[test]
fn source_guard_allows_only_true_absence_and_rejects_modified_or_dangling() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join(native::SOURCE);
    assert!(native::sources(temp.path()).unwrap().live.is_empty());
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"modified").unwrap();
    assert!(native::sources(temp.path()).is_err());
    fs::remove_file(&target).unwrap();
    symlink("absent", &target).unwrap();
    assert!(native::sources(temp.path()).is_err());
    fs::remove_file(&target).unwrap();
    let historical = native::sources(temp.path()).unwrap();
    assert_eq!(historical.historical[native::SOURCE], native::SOURCE_SHA);
    fs::write(
        &target,
        include_bytes!("fixtures/registration-clock/source-original.txt"),
    )
    .unwrap();
    assert_eq!(
        native::sources(temp.path()).unwrap().live[native::SOURCE],
        native::SOURCE_SHA
    );
}
#[test]
fn cancellation_before_child_never_publishes_and_timeout_retains_original_no_report_phase() {
    let temp = tempfile::tempdir().unwrap();
    let o = options(probe(), temp.path().join("report"));
    let cancelled = AtomicBool::new(true);
    assert!(native::run_controlled(&o, &cancelled).is_err());
    assert!(!o.output.exists());
    let mut o = options("/bin/sh".into(), temp.path().join("timeout"));
    // Immutable shell executable; the dynamic body is an argument, never a
    // concurrently rewritten executable file.
    o.runner = vec!["/bin/sh".into(), "-c".into(), "sleep 30".into()];
    o.timeout = Duration::from_millis(50);
    let started = std::time::Instant::now();
    assert!(native::run_controlled(&o, &AtomicBool::new(false)).is_err());
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!o.output.exists());
}
#[test]
fn real_native_cli_help_success_and_child_failure_match_original_phases() {
    let checker = env!("CARGO_BIN_EXE_check-registration-clock");
    for case in fixture("cli-original.json")["cases"].as_array().unwrap() {
        let args = case["args_bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                OsString::from_vec(
                    v.as_array()
                        .unwrap()
                        .iter()
                        .map(|b| b.as_u64().unwrap() as u8)
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        let result = std::process::Command::new(checker)
            .args(args)
            .output()
            .unwrap();
        let expected = |key: &str| {
            String::from_utf8(
                case[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect(),
            )
            .unwrap()
            .replace("check_registration_clock.py", "check-registration-clock")
        };
        assert_eq!(
            result.status.code(),
            Some(case["code"].as_i64().unwrap() as i32)
        );
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            expected("stdout_bytes")
        );
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            expected("stderr_bytes")
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("report");
    let result = std::process::Command::new(checker)
        .arg(probe())
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, native::SUCCESS.as_bytes());
    let report: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(report["source"], fixture("policy-original.json")["source"]);
    fs::remove_file(&output).unwrap();
    let result = std::process::Command::new(checker)
        .arg("/bin/false")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(!output.exists());
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("child exited 1"));
}
#[test]
#[cfg(target_os = "linux")]
fn actual_blocked_final_output_cancellation_cannot_leave_a_pass_report() {
    use rustix::{
        fs::{mkfifoat, open, Mode, OFlags, CWD},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        io::Read,
        process::{Child, Command, Stdio},
        time::Instant,
    };
    struct Owned(Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("report-fifo");
    mkfifoat(CWD, &output, Mode::RUSR | Mode::WUSR).unwrap();
    let stderr = temp.path().join("stderr");
    let mut child = Owned(
        Command::new(env!("CARGO_BIN_EXE_check-registration-clock"))
            .arg(probe())
            .arg(&output)
            .env("OPENPILOT_RUST_ROOT", root())
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
            .spawn()
            .unwrap(),
    );
    let identity = pidfd_open(Pid::from_child(&child.0), PidfdFlags::empty()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fs::read_to_string(format!("/proc/{}/wchan", child.0.id()))
        .is_ok_and(|s| s.trim() == "wait_for_partner")
    {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "checker exited before publication: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "actual blocked publication boundary absent"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    pidfd_send_signal(&identity, Signal::INT).unwrap();
    // Release the actual blocked open only after sending the real signal.
    let mut reader = fs::File::from(
        open(
            &output,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap(),
    );
    let mut bytes = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        let mut buffer = [0; 8192];
        match reader.read(&mut buffer) {
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => panic!("{e}"),
        }
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "cancelled publication stuck");
        std::thread::sleep(Duration::from_millis(5));
    };
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        report["equal"], true,
        "actual final PASS write not exercised"
    );
    assert_eq!(status.code(), Some(130));
    assert!(!output.exists(), "cancelled write retained PASS");
}

struct AfterRealProbe<F: FnMut()> {
    mutation: F,
}
impl<F: FnMut()> openpilot_dev_tools::qa_process::Runner for AfterRealProbe<F> {
    fn run(
        &mut self,
        spec: &openpilot_dev_tools::qa_process::CommandSpec,
    ) -> openpilot_dev_tools::seal::Result<openpilot_dev_tools::qa_process::Captured> {
        let result = NativeRunner.run(spec)?;
        assert_eq!(
            result.code, 0,
            "real probe must execute successfully before mutation"
        );
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).lines().count(),
            13,
            "mutation must not replace the real probe response"
        );
        (self.mutation)();
        Ok(result)
    }
}
fn copied_source(root: &Path) -> PathBuf {
    let source = root.join(native::SOURCE);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        include_bytes!("fixtures/registration-clock/source-original.txt"),
    )
    .unwrap();
    source
}
#[test]
fn real_probe_rows_cannot_be_rebound_to_a_substituted_binary_path() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("probe");
    fs::hard_link(probe(), &binary).unwrap();
    let o = options(binary.clone(), temp.path().join("report.json"));
    let mut runner = AfterRealProbe {
        mutation: || {
            let new = temp.path().join("replacement");
            symlink("/bin/false", &new).unwrap();
            fs::rename(new, &binary).unwrap();
        },
    };
    let result = native::run_with(&o, &mut runner);
    assert!(
        result.is_err(),
        "real A rows were reported with replacement B identity"
    );
    assert!(!o.output.exists());
}
#[test]
fn source_contents_and_presence_are_bound_across_the_real_probe() {
    for mode in ["changed", "removed", "introduced"] {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("root");
        let source = copied_source(&repo);
        if mode == "introduced" {
            fs::remove_file(&source).unwrap();
        }
        let mut o = options(probe(), temp.path().join("report.json"));
        o.root = repo;
        let mut runner = AfterRealProbe {
            mutation: || match mode {
                "removed" => fs::remove_file(&source).unwrap(),
                _ => fs::write(&source, b"modified source\n").unwrap(),
            },
        };
        assert!(
            native::run_with(&o, &mut runner).is_err(),
            "{mode} source was silently accepted"
        );
        assert!(!o.output.exists(), "{mode} source published PASS");
    }
}
#[test]
fn stale_pass_is_invalidated_before_source_failure_without_erasing_failure_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("root");
    let source = copied_source(&repo);
    fs::write(source, b"invalid source").unwrap();
    for (name, bytes, retained) in [
        ("pass", b"{\"equal\":true}\n".as_slice(), false),
        (
            "failure",
            b"{\"equal\":false,\"failure\":\"retain\"}\n".as_slice(),
            true,
        ),
        (
            "other",
            b"arbitrary original failure artifact\n".as_slice(),
            true,
        ),
    ] {
        let output = temp.path().join(name);
        fs::write(&output, bytes).unwrap();
        let mut o = options(probe(), output.clone());
        o.root = repo.clone();
        let result = native::run_with(&o, &mut NativeRunner);
        assert!(result.is_err());
        assert_eq!(output.exists(), retained, "{name}");
        if retained {
            assert_eq!(fs::read(output).unwrap(), bytes);
        }
    }
}
#[test]
fn fifo_binary_keeps_the_real_spawn_error_without_an_eager_hash_read() {
    use rustix::fs::{mkfifoat, Mode, CWD};
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("fifo");
    mkfifoat(CWD, &binary, Mode::RUSR | Mode::WUSR | Mode::XUSR).unwrap();
    let original = fixture("fifo-original.json");
    assert_eq!(original["outcome"], "PermissionError");
    let o = options(binary, temp.path().join("report.json"));
    let start = std::time::Instant::now();
    let error = native::run_with(&o, &mut NativeRunner)
        .err()
        .expect("FIFO cannot execute");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "pre-hash blocked on a nonregular artifact"
    );
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        original["errno"].as_i64().map(|n| n as i32)
    );
    assert!(!o.output.exists());
}
#[test]
fn original_inline_runner_value_with_whitespace_is_admitted_and_executes_the_probe() {
    let original = fixture("runner-inline-original.json");
    let temp = tempfile::tempdir().unwrap();
    let mut args = vec![
        probe().into_os_string(),
        temp.path().join("report.json").into_os_string(),
    ];
    args.extend(
        original["runner_args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| OsString::from(v.as_str().unwrap())),
    );
    let native::Command::Run(o) =
        native::parse(&args, &root(), std::env::vars_os().collect()).unwrap()
    else {
        panic!("run")
    };
    let report = native::run_with(&o, &mut NativeRunner).unwrap();
    assert_eq!(
        as_value(&report)["source"],
        fixture("policy-original.json")["source"]
    );
    assert_eq!(as_value(&report)["equal"], true);
}
