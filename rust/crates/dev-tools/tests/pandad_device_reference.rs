//! Mandatory live original/C++/native cloud comparison. No local probes are run.
#![cfg(all(target_os = "linux", feature = "pandad-device-native"))]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory cloud prerequisite: {name}")),
    );
    assert!(
        path.is_absolute() && path.exists(),
        "missing cloud prerequisite: {name}"
    );
    path
}
fn evidence() -> PathBuf {
    tempfile::Builder::new()
        .prefix("device-pair-")
        .tempdir_in(required("PANDAD_DEVICE_TEST_OUTPUT"))
        .unwrap()
        .keep()
}
fn checker() -> PathBuf {
    env!("CARGO_BIN_EXE_check-pandad-device").into()
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn command(argv: Vec<OsString>) -> CommandSpec {
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(180));
    command.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        root().to_str().unwrap().into(),
    );
    command.os_env = Some(
        [
            ("OPENPILOT_RUST_ROOT".into(), root().into_os_string()),
            (
                "PYTHONPATH".into(),
                std::env::join_paths([root(), root().join("rust/tools")]).unwrap(),
            ),
        ]
        .into_iter()
        .collect(),
    );
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    command
}
fn invocation(original: bool, source: &Path, native: &Path, output: &Path) -> Vec<OsString> {
    let mut argv = if original {
        vec![
            required("PANDAD_DEVICE_PYTHON").into_os_string(),
            "-P".into(),
            root()
                .join("rust/tools/check_pandad_device.py")
                .into_os_string(),
        ]
    } else {
        vec![checker().into_os_string()]
    };
    argv.extend([
        "--source".into(),
        source.into(),
        "--native".into(),
        native.into(),
        "--output".into(),
        output.into(),
    ]);
    argv
}
#[test]
fn unchanged_original_and_native_match_every_dynamic_request_response_and_report() {
    let evidence = evidence();
    let source = required("PANDAD_DEVICE_SOURCE_BINARY");
    let native = required("PANDAD_DEVICE_NATIVE_BINARY");
    let before = [hash(&source), hash(&native)];
    let original = evidence.join("original");
    let output = evidence.join("native");
    for (is_original, directory) in [(true, &original), (false, &output)] {
        let mut spec = command(invocation(is_original, &source, &native, directory));
        if !is_original {
            spec.os_env
                .as_mut()
                .unwrap()
                .insert("PATH".into(), "/nonexistent".into());
        }
        let child = NativeRunner.run(&spec).unwrap();
        let lane = if is_original { "original" } else { "native" };
        fs::write(evidence.join(format!("{lane}.stdout")), &child.stdout).unwrap();
        fs::write(evidence.join(format!("{lane}.stderr")), &child.stderr).unwrap();
        assert_eq!(
            child.code,
            0,
            "{lane}: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert_eq!(
            value(&directory.join("report.json")),
            serde_json::from_slice::<Value>(&child.stdout).unwrap()
        );
    }
    assert_eq!(
        value(&original.join("report.json")),
        value(&output.join("report.json"))
    );
    let report = value(&output.join("report.json"));
    assert_eq!(report["scenarios"], 1746);
    assert_eq!(report["operations"], 2490);
    // No current native request generator participates on the expected side:
    // Python's original Random(175) generator independently creates this file.
    let parse_rows = |path: &Path| {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        parse_rows(&original.join("input.jsonl")),
        parse_rows(&output.join("input.jsonl"))
    );
    for file in [
        "source.jsonl",
        "source.stderr",
        "native.jsonl",
        "native.stderr",
    ] {
        assert_eq!(
            fs::read(original.join(file)).unwrap(),
            fs::read(output.join(file)).unwrap(),
            "full raw {file}"
        );
    }
    assert_eq!(hash(&source), before[0]);
    assert_eq!(hash(&native), before[1]);
    let proof = value(&output.join("provenance.json"));
    assert_eq!(proof["source_executable_sha256"], before[0]);
    assert_eq!(proof["native_runner_executable_sha256"], before[1]);
    let build = value(&required("PANDAD_DEVICE_SOURCE_MANIFEST"));
    assert_eq!(build["binary_sha256"], before[0]);
    assert_eq!(Path::new(build["binary"].as_str().unwrap()), source);
    let sources = build["sources"].as_object().unwrap();
    assert!(sources.contains_key("rust/tools/pandad_device_source.cc"));
    assert!(sources.contains_key("openpilot/selfdrive/pandad/panda.cc"));
    for (name, expected) in sources {
        assert_eq!(
            hash(&root().join(name)),
            expected.as_str().unwrap(),
            "{name}"
        );
    }
}
#[test]
fn unchanged_original_and_native_cli_admission_and_source_first_failure_artifacts() {
    let evidence = evidence();
    let python = required("PANDAD_DEVICE_PYTHON");
    for args in [
        vec![],
        vec!["--help"],
        vec!["--hel"],
        vec!["-hfoo"],
        vec!["-h", "-hfoo"],
        vec!["--unknown"],
        vec!["--source"],
        vec!["--source", "s", "--native", "n"],
        vec!["--runner", "--help"],
    ] {
        let mut outcomes = Vec::new();
        for original in [true, false] {
            let mut argv = if original {
                vec![
                    python.clone().into_os_string(),
                    "-P".into(),
                    root()
                        .join("rust/tools/check_pandad_device.py")
                        .into_os_string(),
                ]
            } else {
                vec![checker().into_os_string()]
            };
            argv.extend(args.iter().map(OsString::from));
            let child = NativeRunner.run(&command(argv)).unwrap();
            outcomes.push(child.code);
        }
        assert_eq!(outcomes[0], outcomes[1], "{args:?}");
        assert!(outcomes[0] == 0 || outcomes[0] == 2);
    }
    let source = required("PANDAD_DEVICE_SOURCE_BINARY");
    let native = required("PANDAD_DEVICE_NATIVE_BINARY");
    for missing_source in [true, false] {
        let absent = evidence.join("absent");
        let mut directories = Vec::new();
        for original in [true, false] {
            let output = evidence.join(format!("missing-{missing_source}-{original}"));
            let argv = invocation(
                original,
                if missing_source { &absent } else { &source },
                if missing_source { &native } else { &absent },
                &output,
            );
            let child = NativeRunner.run(&command(argv)).unwrap();
            assert_eq!(child.code, 1);
            assert!(!output.join("report.json").exists());
            directories.push(output);
        }
        for file in [
            "input.jsonl",
            "source.jsonl",
            "source.stderr",
            "native.jsonl",
            "native.stderr",
        ] {
            assert_eq!(
                directories[0].join(file).exists(),
                directories[1].join(file).exists(),
                "{missing_source}: {file}"
            );
        }
    }
}
