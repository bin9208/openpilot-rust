#![cfg(feature = "paramsd-fixture-native")]
use openpilot_dev_tools::paramsd_fixture_generate::{
    bits, car_params, daemon_case, generate, pose, scenarios, Pose,
};
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}
fn value(path: &std::path::Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn structure_and_boundary_inputs_preserve_original_roster() {
    let scenarios = scenarios();
    let cases = scenarios["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    assert_eq!(cases[0]["name"], "physical-model-0");
    assert_eq!(cases[3]["name"], "active-fallback-and-thresholds");
    assert_eq!(cases[4]["name"], "clipping-hysteresis-and-float32-validity");
    assert_eq!(cases[5]["name"], "nonfinite-state-keeps-prereset-std");
    assert_eq!(cases[6]["name"], "rewind-eviction-and-inactive-clock-reset");
    for profile in &cases[..3] {
        assert_eq!(profile["operations"].as_array().unwrap().len(), 321);
    }
    assert_eq!(cases[3]["operations"].as_array().unwrap().len(), 29);
    assert_eq!(cases[4]["operations"].as_array().unwrap().len(), 64);
    assert_eq!(cases[5]["operations"].as_array().unwrap().len(), 4);
    assert_eq!(cases[6]["operations"].as_array().unwrap().len(), 136);
    assert_eq!(cases[5]["operations"][0]["state"][2], f64::NAN.to_bits(),);
    assert_eq!(
        bits(&[0., -0., f64::INFINITY]),
        [0, 1_u64 << 63, f64::INFINITY.to_bits()]
    );
    assert_ne!(car_params(0, None), car_params(1, None));
    assert_ne!(
        pose(200., Pose::default()),
        pose(
            200.,
            Pose {
                yaw_valid: false,
                ..Pose::default()
            }
        )
    );
    let continuous = daemon_case(false, None).unwrap();
    assert_eq!(continuous["frames"].as_array().unwrap().len(), 1203);
    assert_eq!(continuous["frames"][0]["messages"], serde_json::json!([]));
    assert_eq!(continuous["gps"], "gpsLocationExternal");
    assert!(daemon_case(true, None).is_err());
    let restart = daemon_case(true, Some(b"original saved bytes")).unwrap();
    assert_eq!(restart["frames"].as_array().unwrap().len(), 9);
    assert_eq!(restart["gps"], "gpsLocation");
    assert_eq!(
        restart["seed"]["LiveParametersV2"],
        serde_json::json!(b"original saved bytes")
    );
}

#[test]
fn all_wire_packets_match_fresh_independent_original_capture() {
    let root = required("PARAMSD_FIXTURE_ROOT");
    let saved = required("PARAMSD_FIXTURE_ORIGINAL_SAVED_PACKET");
    let original_scenarios = required("PARAMSD_FIXTURE_ORIGINAL_SCENARIOS");
    let original_continuous = required("PARAMSD_FIXTURE_ORIGINAL_CONTINUOUS");
    let original_restart = required("PARAMSD_FIXTURE_ORIGINAL_RESTART");
    let native_output = required("PARAMSD_FIXTURE_NATIVE_OUTPUT");
    let native_exec = required("PARAMSD_FIXTURE_NATIVE_EXEC");
    let native_binary = required("PARAMSD_FIXTURE_NATIVE_BINARY");
    assert!(native_binary.is_file(), "cloud native generator missing");
    let trace = fs::read_to_string(native_exec).unwrap();
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    assert_eq!(
        opening.find_iter(&trace).count(),
        1,
        "extra native exec attempt"
    );
    let invocation =
        regex::Regex::new(r#"(?m)\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let captures = invocation
        .captures(&trace)
        .expect("one successful native execve");
    let program: String = serde_json::from_str(&captures[1]).unwrap();
    let argv: Vec<String> = serde_json::from_str(&captures[2]).unwrap();
    assert_eq!(program, native_binary.to_str().unwrap());
    assert_eq!(
        argv,
        [
            program,
            "--root".into(),
            root.to_str().unwrap().into(),
            "--output".into(),
            native_output.to_str().unwrap().into(),
            "--saved-packet".into(),
            saved.to_str().unwrap().into(),
        ]
    );
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("native");
    generate(&root, &output, Some(&saved)).unwrap();
    for (name, original) in [
        ("scenarios.json", original_scenarios),
        ("continuous-input.json", original_continuous),
        ("restart-input.json", original_restart),
    ] {
        assert_eq!(value(&output.join(name)), value(&original), "{name}");
        assert_eq!(
            value(&native_output.join(name)),
            value(&original),
            "CLI {name}"
        );
        assert_eq!(
            fs::read(output.join(name)).unwrap(),
            fs::read(native_output.join(name)).unwrap(),
            "library/CLI {name}"
        );
    }
}

#[test]
fn cli_rejects_missing_duplicate_unknown_and_existing_output() {
    let binary = env!("CARGO_BIN_EXE_generate-paramsd-fixture");
    let temporary = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = temporary.path().join("generated");
    let negative = [
        (
            vec!["--root".into(), root.as_os_str().to_owned()],
            "--output required",
        ),
        (
            vec![
                "--root".into(),
                root.as_os_str().to_owned(),
                "--root".into(),
                root.as_os_str().to_owned(),
            ],
            "duplicate option",
        ),
        (vec!["--unknown".into(), "value".into()], "unknown option"),
    ];
    for (args, expected) in negative {
        let result = Command::new(binary).args(args).output().unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected),
            "{expected}"
        );
        assert!(!output.exists());
    }
    fs::create_dir(&output).unwrap();
    let result = Command::new(binary)
        .args([
            "--root",
            root.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("File exists"));
    assert_eq!(
        fs::read_dir(output).unwrap().count(),
        0,
        "existing output modified"
    );
}
