use openpilot_dev_tools::camera_source_generate::{generate, Kind};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const FILES: [&str; 4] = [
    "openpilot/system/camerad/cameras/spectra.cc",
    "openpilot/system/camerad/cameras/ife.h",
    "rust/tools/camerad_requests_source.cc.in",
    "rust/tools/camerad_packets_source.cc.in",
];
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn source_tree(path: &Path, kind: &str, mode: &str) {
    let template = if kind == "requests" { 2 } else { 3 };
    for (index, name) in FILES.iter().enumerate() {
        let mut data = fs::read_to_string(root().join(name)).unwrap();
        if mode == "crlf" {
            data = data.replace('\n', "\r\n");
        }
        if mode == "cr" {
            data = data.replace('\n', "\r");
        }
        if index == template {
            match mode {
                "trailing-python-space" => {
                    data.push_str("\n\u{1c}\u{1d}\u{1e}\u{1f}\u{85}\u{2003}\u{3000}")
                }
                "duplicate-marker" => data.push_str("\n@METHODS@\n"),
                "missing-marker" => data = data.replace("@METHODS@", "NO_METHODS"),
                "empty-template" => data.clear(),
                _ => {}
            }
        }
        if index == 0 {
            match mode {
                "missing-method" => {
                    data = data.replace(
                        if kind == "requests" {
                            "handle_camera_event("
                        } else {
                            "power_set_wait("
                        },
                        "not_the_method(",
                    )
                }
                "missing-terminator" => data = data.replace("\n}", "\n }"),
                "changed-method" => {
                    let old = if kind == "requests" {
                        "bool SpectraCamera::handle_camera_event("
                    } else {
                        "void add_patch("
                    };
                    data = data.replace(old, &format!("// preceding comment\n{old}"));
                }
                "missing-port" => {
                    data = data.replace(
                        "  struct cam_isp_in_port_info in_port_info =",
                        "  omitted_port =",
                    )
                }
                "method-semicolon" => data = data.replace("\n}", "\n};"),
                _ => {}
            }
        }
        if index == 1 && mode == "missing-ife" {
            data = "no function\n".into();
        }
        let mut data = data.into_bytes();
        if (index == 0 && mode == "invalid-source")
            || (index == template && mode == "invalid-template")
        {
            data.push(255);
        }
        let target = path.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, data).unwrap();
    }
}

#[test]
fn original_camera_generator_outputs_and_failures_on_changed_source_trees() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-generators/original.json")).unwrap();
    for name in FILES {
        assert_eq!(
            sha(&fs::read(root().join(name)).unwrap()),
            original["sources"][name]
        );
    }
    for case in original["cases"].as_array().unwrap() {
        let kind = case["kind"].as_str().unwrap();
        let mode = case["mode"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        source_tree(dir.path(), kind, mode);
        let result = generate(
            dir.path(),
            if kind == "requests" {
                Kind::Requests
            } else {
                Kind::Packets
            },
        );
        assert_eq!(
            result.is_ok(),
            case["exit"] == 0,
            "{kind}/{mode}: {result:?}"
        );
        let output = result.unwrap_or_default();
        assert_eq!(
            output.len() as u64,
            case["stdout_bytes"].as_u64().unwrap(),
            "{kind}/{mode}"
        );
        assert_eq!(
            sha(output.as_bytes()),
            case["stdout_sha256"],
            "{kind}/{mode}"
        );
    }
}

#[test]
fn actual_native_cli_needs_no_interpreter_and_rejects_invalid_arguments() {
    for (kind, binary) in [
        (
            Kind::Requests,
            env!("CARGO_BIN_EXE_generate-camerad-requests-reference"),
        ),
        (
            Kind::Packets,
            env!("CARGO_BIN_EXE_generate-camerad-packet-reference"),
        ),
    ] {
        let child = Command::new(binary)
            .arg("--source")
            .arg(root())
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert!(child.status.success(), "{:?}", child.stderr);
        assert_eq!(child.stdout, generate(&root(), kind).unwrap().as_bytes());
        for args in [vec![], vec!["--source"], vec!["--source", ".", "--unknown"]] {
            let invalid = Command::new(binary).args(args).output().unwrap();
            assert_eq!(invalid.status.code(), Some(2));
            assert!(invalid.stdout.is_empty());
        }
    }
}

#[test]
fn original_argparse_aliases_repeats_and_exit_classes() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-generators/cli.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let binary = if case["kind"] == "requests" {
            env!("CARGO_BIN_EXE_generate-camerad-requests-reference")
        } else {
            env!("CARGO_BIN_EXE_generate-camerad-packet-reference")
        };
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| {
                arg.as_str()
                    .unwrap()
                    .replace("$ROOT", root().to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let output = Command::new(binary).args(args).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{case}"
        );
        assert_eq!(
            !output.stdout.is_empty(),
            case["stdout"].as_bool().unwrap(),
            "{case}"
        );
        assert_eq!(
            !output.stderr.is_empty(),
            case["stderr"].as_bool().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn camera_ci_builds_and_uses_native_generators_before_original_comparisons() {
    let source = fs::read_to_string(root().join("rust/tools/check_camerad_ci.py")).unwrap();
    for name in [
        "generate-camerad-requests-reference",
        "generate-camerad-packet-reference",
        "generate-camerad-{kind}-data",
        "generate-camerad-ae-reference",
        "generate-camerad-state-reference",
        "generate-camerad-stress-reference",
        "generate-camerad-runtime-fixture",
        "check-camerad-{checker}",
        "check-camerad-kernel",
        "check-camerad-sensor-lifecycle",
    ] {
        assert!(source.contains(name), "camera caller omits {name}");
    }
    let workflow = openpilot_dev_tools::ci_policy::parse(
        &fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap(),
    )
    .unwrap();
    let steps = workflow["jobs"]["camera-runtime"]["steps"]
        .as_array()
        .unwrap();
    let native = steps
        .iter()
        .position(|step| step["name"] == "Build and validate native camera source generators")
        .unwrap();
    let compare = steps
        .iter()
        .position(|step| {
            step["name"] == "Require original camera comparisons and continuous driver fixtures"
        })
        .unwrap();
    assert!(native < compare);
    assert!(steps[native].get("if").is_none());
    assert!(steps[native].get("continue-on-error").is_none());
    let run = steps[native]["run"].as_str().unwrap();
    for required in [
        "--growth-mib 1024",
        "--test camera_source_generate",
        "--test camera_data_generate",
        "--test camera_sensor_generate",
        "--test camera_sensor_check",
        "--test camera_request_check",
        "--bin check-camerad-requests",
        "--test camera_exposure_check",
        "--test camera_misc_check",
        "--bin check-camerad-exposure",
        "--bin check-camerad-misc",
        "--test camera_packet_check",
        "--bin check-camerad-packets",
        "--test camera_kernel_check",
        "--bin check-camerad-kernel",
        "--test camera_sensor_lifecycle_check",
        "--bin check-camerad-sensor-lifecycle",
        "--bin check-camerad-sensors",
        "--features camera-sensor-native",
        "--test camera_lifecycle_generate",
        "--bin generate-camerad-requests-reference",
        "--bin generate-camerad-packet-reference",
        "--bin generate-camerad-bps-data",
        "--bin generate-camerad-sensor-data",
        "--bin generate-camerad-ae-reference",
        "--bin generate-camerad-kernel-reference",
        "--bin generate-camerad-sensor-lifecycle-reference",
        "--bin generate-camerad-isp-lifecycle-reference",
        "--bin generate-camerad-camera-lifecycle-reference",
        "--bin generate-camerad-state-reference",
        "--bin generate-camerad-stress-reference",
        "--bin generate-camerad-runtime-fixture",
        "--locked",
        "-D warnings",
    ] {
        assert!(run.contains(required), "missing camera gate {required}");
    }
}
