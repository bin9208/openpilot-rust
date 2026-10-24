#![cfg(feature = "paramsd-fixture-native")]
use openpilot_cereal::car_capnp::car_params;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn car_params_cli_preserves_bytes_and_rejects_foreign_output_and_invalid_arguments() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_generate-paramsd-car-params");
    let output = temporary.path().join("CarParams.bin");
    let result = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let message =
        capnp::serialize::read_message(&bytes[..], capnp::message::ReaderOptions::new()).unwrap();
    let cp = message.get_root::<car_params::Reader<'_>>().unwrap();
    assert_eq!(
        cp.get_car_fingerprint().unwrap().to_str().unwrap(),
        "SYNTHETIC_0"
    );
    assert_eq!(cp.get_mass().to_bits(), 1600_f32.to_bits());
    assert_eq!(cp.get_steer_ratio().to_bits(), 15_f32.to_bits());
    let second = Command::new(binary)
        .arg("--root")
        .arg(&root)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);
    for arguments in [
        vec!["--profile", "3"],
        vec!["--profile", "-1"],
        vec!["--profile", "x"],
        vec!["--root", "/"],
        vec!["--unknown", "x"],
    ] {
        let missing = temporary.path().join("missing");
        let result = Command::new(binary)
            .arg("--root")
            .arg(&root)
            .arg("--output")
            .arg(&missing)
            .args(arguments)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!missing.exists());
    }
}

#[test]
#[ignore = "hosted exact original CarParams CLI gate only"]
fn all_cli_profiles_and_fingerprints_match_fresh_python_fixture_bytes() {
    let proof =
        PathBuf::from(std::env::var_os("PARAMSD_CAR_PARAMS_PROOF").expect("hosted proof required"));
    for profile in 0..3 {
        for name in ["default", "empty", "custom"] {
            assert_eq!(
                fs::read(proof.join(format!("original-{profile}-{name}.bin"))).unwrap(),
                fs::read(proof.join(format!("native-{profile}-{name}.bin"))).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "hosted maintained atomic-gate exec audit only"]
fn maintained_atomic_gate_uses_exact_native_generator_exec() {
    let proof = PathBuf::from(
        std::env::var_os("PARAMSD_CAR_PARAMS_GATE_PROOF").expect("atomic gate proof required"),
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let binary = fs::canonicalize(env!("CARGO_BIN_EXE_generate-paramsd-car-params")).unwrap();
    let trace = fs::read_to_string(proof.join("car-params.exec")).unwrap();
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    assert_eq!(
        opening.find_iter(&trace).count(),
        1,
        "extra/failed exec attempt"
    );
    let invocation =
        regex::Regex::new(r#"(?m)\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let fields = invocation
        .captures(&trace)
        .expect("exact successful native exec required");
    let program: String = serde_json::from_str(&fields[1]).unwrap();
    let argv: Vec<String> = serde_json::from_str(&fields[2]).unwrap();
    assert_eq!(program, binary.to_str().unwrap());
    assert_eq!(
        argv,
        [
            program,
            "--root".into(),
            root.to_str().unwrap().into(),
            "--output".into(),
            proof.join("CarParams.bin").to_str().unwrap().into()
        ]
    );
    assert_eq!(
        fs::read(proof.join("CarParams.bin")).unwrap(),
        openpilot_dev_tools::paramsd_fixture_generate::car_params(0, None)
    );
}
