use openpilot_dev_tools::{
    params_reference_check::ParamsRunner,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
    selfdrive_params_check::{build_reference, check},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
struct BuildPeer {
    calls: usize,
    fail: usize,
}
struct LiveBuild;
impl Runner for LiveBuild {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        NativeRunner.run(spec)
    }
}
impl ParamsRunner for LiveBuild {
    fn locked_put(&mut self, _: &Path, _: &Path) -> Result<()> {
        Err("unused build locking seam".into())
    }
}

#[test]
fn real_original_cpp_and_native_transport_peer_preserve_the_captured_matrix() {
    let temp = tempfile::tempdir().unwrap();
    let stores = tempfile::Builder::new().prefix("sdp-").tempdir().unwrap();
    let prefix = std::env::var_os("SELFDRIVE_PARAMS_CAPNP_PREFIX").map(PathBuf::from);
    let reference = build_reference(
        &root(),
        &temp.path().join("reference"),
        prefix.as_deref(),
        &mut LiveBuild,
    )
    .unwrap();
    let boundaries: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/selfdrive-params/boundaries.json")).unwrap();
    let mut matrix = vec![
        serde_json::json!({"mode":"normal","success":true,"stdout":"{\"passed\": true, \"cases\": 47}\n"}),
    ];
    matrix.extend(boundaries["cases"].as_array().unwrap().iter().cloned());
    for expected in matrix {
        let mode = expected["mode"].as_str().unwrap();
        let output = temp.path().join(mode);
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-selfdrive-params"))
            .args([
                "--native",
                env!("CARGO_BIN_EXE_selfdrive-params-test-peer"),
                "--reference",
            ])
            .arg(&reference)
            .arg("--output")
            .arg(&output)
            .env("OPENPILOT_RUST_ROOT", root())
            .env("TMPDIR", stores.path())
            .env("PATH", "/nonexistent")
            .env("SELFDRIVE_PARAMS_PEER_MODE", mode)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert_eq!(
            result.status.success(),
            expected["success"].as_bool().unwrap(),
            "{mode}: {stderr}"
        );
        assert_eq!(
            String::from_utf8_lossy(&result.stdout),
            expected["stdout"].as_str().unwrap(),
            "{mode}"
        );
        assert_eq!(
            fs::read_dir(stores.path()).unwrap().count(),
            0,
            "{mode}: temporary cleanup"
        );
        let mut artifacts = fs::read_dir(&output)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        artifacts.sort();
        if mode != "normal" {
            assert_eq!(
                serde_json::json!(artifacts),
                expected["artifacts"],
                "{mode}: partial artifacts"
            );
            let warnings = stderr
                .lines()
                .filter(|line| line.starts_with("Failed to cast param "))
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::json!(warnings),
                expected["warnings"],
                "{mode}: warning order"
            );
        }
        if result.status.success() {
            assert_eq!(artifacts.len(), 9, "{mode}");
            let report = openpilot_log_format::JsonValue::parse(
                &fs::read_to_string(output.join("report.json")).unwrap(),
            )
            .unwrap();
            let cases = report.get("cases").unwrap();
            let openpilot_log_format::JsonView::Array(cases) = cases.view() else {
                panic!("missing cases");
            };
            assert_eq!(cases.len(), 47);
            assert_eq!(report.get("passed").unwrap().to_json().unwrap(), "true");
        }
    }
}
impl Runner for BuildPeer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        if self.calls == self.fail {
            return Ok(Captured {
                code: 7,
                stdout: vec![],
                stderr: vec![],
            });
        }
        let argv = spec.effective_argv();
        if let Some(index) = argv.iter().position(|arg| arg == "-o") {
            fs::write(Path::new(&argv[index + 1]), b"independent compiler peer")?;
        }
        Ok(Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
impl ParamsRunner for BuildPeer {
    fn locked_put(&mut self, _: &Path, _: &Path) -> Result<()> {
        Err("unused".into())
    }
}
#[test]
fn failed_reference_build_cannot_publish_receipt_or_reuse_output() {
    for fail in [1, 2, 3] {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("source");
        let mut peer = BuildPeer { calls: 0, fail };
        assert!(build_reference(&root(), &output, None, &mut peer).is_err());
        assert!(!output
            .join("selfdrive-params-reference.provenance.json")
            .exists());
        assert!(build_reference(&root(), &output, None, &mut peer).is_err());
        assert_eq!(peer.calls, fail);
    }
}
#[test]
fn reference_build_receipt_pins_unchanged_sources_and_helper() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("source");
    let mut peer = BuildPeer {
        calls: 0,
        fail: usize::MAX,
    };
    let binary = build_reference(&root(), &output, None, &mut peer).unwrap();
    assert_eq!(peer.calls, 3);
    assert!(binary.exists());
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("selfdrive-params-reference.provenance.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["kind"], "selfdrive-params-original-cpp");
    assert!(receipt["sources"]
        .get("openpilot/common/params.cc")
        .is_some());
    assert!(receipt["sources"]
        .get("rust/crates/dev-tools/tests/fixtures/selfdrive-params/selfdrive_params_reference.cc")
        .is_some());
}
#[test]
fn missing_reference_receipt_and_live_source_drift_do_not_spawn_or_create_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let reference = temp.path().join("reference");
    fs::write(&reference, b"fake").unwrap();
    let output = temp.path().join("output");
    let mut peer = BuildPeer {
        calls: 0,
        fail: usize::MAX,
    };
    assert!(check(
        &root(),
        Path::new("candidate"),
        &reference,
        &output,
        &mut peer,
        &mut vec![]
    )
    .is_err());
    assert!(!output.exists());
    assert_eq!(peer.calls, 0);
    let m: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/selfdrive-params/manifest.json")).unwrap();
    for name in m["sources"].as_object().unwrap().keys() {
        let target = temp.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(root().join(name), target) {
            Err(error)
                if name == "rust/tools/check_selfdrive_params.py"
                    && error.kind() == std::io::ErrorKind::NotFound => {}
            result => {
                result.unwrap();
            }
        }
    }
    fs::copy(root().join("uv.lock"), temp.path().join("uv.lock")).unwrap();
    for name in m["sources"].as_object().unwrap().keys() {
        let target = temp.path().join(name);
        let original = fs::read(&target).ok();
        if original.is_none() {
            assert_eq!(name, "rust/tools/check_selfdrive_params.py");
        }
        fs::write(&target, b"changed source").unwrap();
        let error = check(
            temp.path(),
            Path::new("candidate"),
            &reference,
            &output,
            &mut peer,
            &mut vec![],
        )
        .unwrap_err();
        assert!(error.to_string().contains(name), "{error}");
        assert!(!output.exists());
        assert_eq!(peer.calls, 0);
        fs::remove_file(&target).unwrap();
        assert!(check(
            temp.path(),
            Path::new("candidate"),
            &reference,
            &output,
            &mut peer,
            &mut vec![]
        )
        .is_err());
        assert!(!output.exists());
        assert_eq!(peer.calls, 0);
        if let Some(original) = original {
            fs::write(&target, original).unwrap();
        }
    }
}
#[test]
fn cli_argument_failures_do_not_create_outputs() {
    for args in [
        vec![],
        vec!["--help"],
        vec!["--native", "candidate"],
        vec!["--reference", "helper"],
        vec!["--unknown"],
    ] {
        let temp = tempfile::tempdir().unwrap();
        let help = args == ["--help"];
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-selfdrive-params"))
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(if help { 0 } else { 2 }));
        assert_eq!(!result.stdout.is_empty(), help);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
