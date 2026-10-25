use std::{fs, path::Path, process::Command};
#[test]
fn independent_source_reference_replays_all_27_original_live_ioctl_fault_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/amplifier-linux");
    let temp = tempfile::tempdir().unwrap();
    openpilot_disk_space::inspect(temp.path(), 16 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let library = temp.path().join("fixture.so");
    let status = Command::new("cc")
        .args([
            "-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC",
        ])
        .arg(root.join("rust/tools/amplifier_i2c_fixture.c"))
        .arg("-ldl")
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap();
    assert!(status.success());
    let helper = temp.path().join("syscalls");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg(fixtures.join("syscalls.c"))
        .arg("-o")
        .arg(&helper)
        .status()
        .unwrap()
        .success());
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures.join("original27.json")).unwrap()).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let folder = temp.path().join(name);
        fs::create_dir(&folder).unwrap();
        let device = folder.join("device");
        if case["missing"] == false {
            fs::write(&device, b"only a regular fixture file").unwrap();
        }
        let actions = serde_json::json!([{"kind":"read","register":81},{"kind":"write","register":81,"value":133},{"kind":"read","register":81},{"kind":"write","register":16,"value":23},{"kind":"read","register":16}]);
        let input = folder.join("input.json");
        fs::write(
            &input,
            serde_json::to_vec(&serde_json::json!({"device":device,"actions":actions})).unwrap(),
        )
        .unwrap();
        let faults = case["faults"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(index, error)| format!("{index}:{error}"))
            .collect::<Vec<_>>()
            .join(",");
        let result = Command::new(env!("CARGO_BIN_EXE_amplifier-linux-reference"))
            .arg(&input)
            .env("AMP_SYSCALL_HELPER", &helper)
            .env("AMP_DEVICE_PATH", &device)
            .env("AMP_TRACE_PATH", folder.join("syscalls.jsonl"))
            .env("AMP_STATE_PATH", folder.join("registers.bin"))
            .env("AMP_SEED", "47")
            .env("AMP_FAULTS", faults)
            .env("LD_PRELOAD", "")
            .env("AMP_SYSCALL_PRELOAD", &library)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let outcome: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(outcome, case["result"]["outcome"], "{name}");
        let trace = folder.join("syscalls.jsonl");
        let events: Vec<serde_json::Value> = if trace.exists() {
            fs::read_to_string(trace)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        } else {
            vec![]
        };
        assert_eq!(
            serde_json::json!(events),
            case["result"]["events"],
            "{name} exact syscalls"
        );
        assert_eq!(
            serde_json::json!(fs::read(folder.join("registers.bin")).unwrap()),
            case["result"]["registers"],
            "{name} registers"
        );
    }
}

#[test]
fn generic_source_actions_preserve_integer_casts_failures_and_context_cleanup() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/amplifier-linux");
    let temp = tempfile::tempdir().unwrap();
    openpilot_disk_space::inspect(temp.path(), 16 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let library = temp.path().join("fixture.so");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC"])
        .arg(root.join("rust/tools/amplifier_i2c_fixture.c"))
        .arg("-ldl")
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap()
        .success());
    let helper = temp.path().join("syscalls");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg(fixtures.join("syscalls.c"))
        .arg("-o")
        .arg(&helper)
        .status()
        .unwrap()
        .success());
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures.join("generic-original.json")).unwrap()).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let folder = temp.path().join(name);
        fs::create_dir(&folder).unwrap();
        let device = folder.join("device");
        fs::write(&device, b"only a regular fixture file").unwrap();
        let device_json = serde_json::to_string(&device).unwrap();
        let config = if case["actions"] == "MISSING" {
            format!("{{\"device\": {device_json}}}")
        } else {
            format!(
                "{{\"device\": {device_json}, \"actions\": {}}}",
                case["actions_json"].as_str().unwrap()
            )
        };
        let input = folder.join("input.json");
        fs::write(&input, config).unwrap();
        let observed = Command::new(env!("CARGO_BIN_EXE_amplifier-linux-reference"))
            .arg(&input)
            .env("AMP_SYSCALL_HELPER", &helper)
            .env("AMP_DEVICE_PATH", &device)
            .env("AMP_TRACE_PATH", folder.join("syscalls.jsonl"))
            .env("AMP_STATE_PATH", folder.join("registers.bin"))
            .env("AMP_SEED", "47")
            .env("AMP_FAULTS", "")
            .env("LD_PRELOAD", "")
            .env("AMP_SYSCALL_PRELOAD", &library)
            .output()
            .unwrap();
        assert_eq!(
            observed.status.code(),
            case["exit"].as_i64().map(|v| v as i32),
            "{name}: {}",
            String::from_utf8_lossy(&observed.stderr)
        );
        assert_eq!(
            observed.stdout,
            case["stdout"].as_str().unwrap().as_bytes(),
            "{name}"
        );
        let trace = folder.join("syscalls.jsonl");
        let events: Vec<serde_json::Value> = if trace.exists() {
            fs::read_to_string(trace)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        } else {
            vec![]
        };
        assert_eq!(
            serde_json::json!(events),
            case["events"],
            "{name} syscall order"
        );
        assert_eq!(
            serde_json::json!(fs::read(folder.join("registers.bin")).unwrap()),
            case["registers"],
            "{name} state"
        );
    }
}

#[test]
fn checker_argument_failures_are_side_effect_free_and_match_original_status() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/amplifier-linux/cli-original.json")).unwrap();
    for row in original.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_check-amplifier-linux"))
            .args(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap()),
            )
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{row}"
        );
        assert_eq!(!output.stdout.is_empty(), row["help"] == true);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn original_checker_retirement_and_reintroduced_source_drift_are_guarded() {
    use openpilot_dev_tools::{
        amplifier_linux_check::check,
        qa_process::{Captured, CommandSpec, Runner},
        seal::Result,
    };
    struct ReachCompiler;
    impl Runner for ReachCompiler {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            assert_eq!(spec.effective_argv().first().unwrap(), "cc");
            Err("reached independent ABI compiler".into())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    for source in [
        "openpilot/common/i2c.py",
        "rust/tools/amplifier_i2c_fixture.c",
    ] {
        let dest = temp.path().join(source);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(root.join(source), dest).unwrap();
    }
    for retired in [
        None,
        Some("rust/tools/check_amplifier_linux.py"),
        Some("rust/tools/amplifier_linux_source.py"),
    ] {
        if let Some(source) = retired {
            let dest = temp.path().join(source);
            fs::write(dest, b"changed reintroduced source").unwrap();
        }
        let result = check(
            temp.path(),
            Path::new("candidate"),
            Path::new("fixture"),
            &temp.path().join("output"),
            &[],
            Some("ordinary"),
            Path::new("reference"),
            &mut ReachCompiler,
            &mut vec![],
        )
        .unwrap_err();
        if let Some(source) = retired {
            assert!(result.to_string().contains("changed"), "{result}");
            fs::remove_file(temp.path().join(source)).unwrap();
        } else {
            assert!(
                result
                    .to_string()
                    .contains("reached independent ABI compiler"),
                "{result}"
            );
        }
    }
}

#[test]
fn pre_open_input_failures_preserve_original_initial_fixture_state_without_ioctls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/amplifier-linux");
    let temp = tempfile::tempdir().unwrap();
    openpilot_disk_space::inspect(temp.path(), 16 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let library = temp.path().join("fixture.so");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC"])
        .arg(root.join("rust/tools/amplifier_i2c_fixture.c"))
        .arg("-ldl")
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap()
        .success());
    let helper = temp.path().join("syscalls");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg(fixtures.join("syscalls.c"))
        .arg("-o")
        .arg(&helper)
        .status()
        .unwrap()
        .success());
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/amplifier-linux/early-original.json")).unwrap();
    for row in original.as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let folder = temp.path().join(name);
        fs::create_dir(&folder).unwrap();
        let input = folder.join("input.json");
        if let Some(text) = row["input"].as_str() {
            fs::write(&input, text).unwrap();
        }
        let device = folder.join("device");
        fs::write(&device, b"fixture").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_amplifier-linux-reference"))
            .arg(input)
            .env("LD_PRELOAD", "")
            .env("AMP_SYSCALL_PRELOAD", &library)
            .env("AMP_SYSCALL_HELPER", &helper)
            .env("AMP_DEVICE_PATH", device)
            .env("AMP_TRACE_PATH", folder.join("syscalls.jsonl"))
            .env("AMP_STATE_PATH", folder.join("registers.bin"))
            .env("AMP_SEED", "47")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{name}"
        );
        assert_eq!(
            output.stdout,
            row["stdout"].as_str().unwrap().as_bytes(),
            "{name}"
        );
        assert!(
            !folder.join("syscalls.jsonl").exists(),
            "{name}: no syscall before admitted open"
        );
        assert_eq!(
            serde_json::json!(fs::read(folder.join("registers.bin")).unwrap()),
            row["registers"],
            "{name}"
        );
    }
}

#[test]
fn empty_case_filter_enters_all_cases_like_original_not_empty_success() {
    use openpilot_dev_tools::{
        amplifier_linux_check::check,
        qa_process::{Captured, CommandSpec, Runner},
        seal::Result,
    };
    struct Peer(usize);
    impl Runner for Peer {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            self.0 += 1;
            if self.0 == 1 {
                assert_eq!(spec.effective_argv()[0], "cc");
                return Ok(Captured {
                    code: 0,
                    stdout: vec![],
                    stderr: vec![],
                });
            }
            assert_eq!(spec.effective_argv()[0], "reference");
            Err("entered original first source case".into())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("out");
    fs::create_dir(&output).unwrap();
    let mut peer = Peer(0);
    let error = check(
        &root,
        Path::new("candidate"),
        Path::new("fixture"),
        &output,
        &[],
        Some(""),
        Path::new("reference"),
        &mut peer,
        &mut vec![],
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("entered original first source case"),
        "{error}"
    );
    assert_eq!(peer.0, 2);
    assert!(output.join("ordinary/source/input.json").exists());
}

#[test]
fn missing_input_argument_still_persists_the_original_preload_initial_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/amplifier-linux");
    let temp = tempfile::tempdir().unwrap();
    openpilot_disk_space::inspect(temp.path(), 16 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let library = temp.path().join("fixture.so");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC"])
        .arg(root.join("rust/tools/amplifier_i2c_fixture.c"))
        .arg("-ldl")
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap()
        .success());
    let helper = temp.path().join("syscalls");
    assert!(Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg(fixtures.join("syscalls.c"))
        .arg("-o")
        .arg(&helper)
        .status()
        .unwrap()
        .success());
    let row: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/amplifier-linux/missing-argument-original.json"
    ))
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_amplifier-linux-reference"))
        .env("LD_PRELOAD", "")
        .env("AMP_SYSCALL_PRELOAD", &library)
        .env("AMP_SYSCALL_HELPER", &helper)
        .env("AMP_STATE_PATH", temp.path().join("registers.bin"))
        .env("AMP_TRACE_PATH", temp.path().join("syscalls.jsonl"))
        .env("AMP_SEED", "47")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), row["exit"].as_i64().map(|v| v as i32));
    assert_eq!(output.stdout, row["stdout"].as_str().unwrap().as_bytes());
    assert_eq!(
        serde_json::json!(fs::read(temp.path().join("registers.bin")).unwrap()),
        row["registers"]
    );
    assert!(!temp.path().join("syscalls.jsonl").exists());
}
