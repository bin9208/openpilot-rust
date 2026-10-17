//! Independent original-helper admission captures, including successful Python coercion.
#![cfg(feature = "image-array-native")]
use serde_json::Value;
use std::{fs, path::Path, process::Command};
#[test]
fn actual_cli_preserves_original_admission_state_pixels_and_failure_artifacts_when_available() {
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let captures = [
        include_str!("fixtures/ui-forms/admission-original.json"),
        include_str!("fixtures/ui-forms/scene-admission-original.json"),
    ];
    let cases = captures
        .iter()
        .flat_map(|source| {
            serde_json::from_str::<Value>(source).unwrap()["cases"]
                .as_array()
                .unwrap()
                .clone()
        })
        .collect::<Vec<_>>();
    for case in &cases {
        let temp = tempfile::tempdir().unwrap();
        let scene = temp.path().join("scene.json");
        let png = temp.path().join("output.png");
        let bytes = case
            .get("scene_text")
            .and_then(Value::as_str)
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_else(|| serde_json::to_vec(&case["scene"]).unwrap());
        fs::write(&scene, bytes).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ui-forms-reference"));
        command
            .args([scene.as_os_str(), png.as_os_str()])
            .env("OPENPILOT_RUST_ROOT", &root)
            .env("UI_FORMS_RAW_HELPER", &helper);
        for key in [
            "RECORD",
            "BURN_IN",
            "PROFILE_STARTUP",
            "PROFILE_RENDER",
            "GRID",
            "SHOW_FPS",
            "SHOW_TOUCHES",
            "RECORD_OUTPUT",
            "RECORD_QUALITY",
            "RECORD_SPEED",
            "PROFILE_STATS",
            "STRICT_MODE",
            "ENABLE_VSYNC",
        ] {
            command.env_remove(key);
        }
        if let Some(environment) = case.get("env").and_then(Value::as_object) {
            for (name, value) in environment {
                command.env(name, value.as_str().unwrap());
            }
        }
        let observed = command.output().unwrap();
        let name = case["name"].as_str().unwrap();
        assert_eq!(
            observed.status.success(),
            case["exit"] == 0,
            "{name}: {}",
            String::from_utf8_lossy(&observed.stderr)
        );
        assert_eq!(png.exists(), case["png"].as_bool().unwrap(), "{name}");
        let state = png.with_extension("json");
        assert_eq!(state.exists(), !case["states"].is_null(), "{name}");
        if state.exists() {
            let observed: Value = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
            // JSON numeric representations may differ while retaining exact numeric values.
            fn same(a: &Value, b: &Value) -> bool {
                match (a, b) {
                    (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
                    (Value::Array(a), Value::Array(b)) => {
                        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
                    }
                    (Value::Object(a), Value::Object(b)) => {
                        a.len() == b.len()
                            && a.iter().all(|(k, v)| b.get(k).is_some_and(|b| same(v, b)))
                    }
                    _ => a == b,
                }
            }
            assert!(
                same(&observed, &case["states"]),
                "{name}: {observed} != {}",
                case["states"]
            );
        }
        if png.exists() {
            use sha2::{Digest, Sha256};
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(png).unwrap())),
                case["png_sha256"].as_str().unwrap(),
                "{name}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn actual_cli_resolves_output_symlink_before_choosing_original_state_path_when_available() {
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let captured: Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/admission-original.json")).unwrap();
    let case = captured["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "float-frame")
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let scene = temp.path().join("scene.json");
    fs::write(&scene, serde_json::to_vec(&case["scene"]).unwrap()).unwrap();
    let target = temp.path().join("target.png");
    fs::write(&target, []).unwrap();
    let alias = temp.path().join("alias.png");
    std::os::unix::fs::symlink("target.png", &alias).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ui-forms-reference"));
    command
        .args([scene.as_os_str(), alias.as_os_str()])
        .env("OPENPILOT_RUST_ROOT", &root)
        .env("UI_FORMS_RAW_HELPER", helper);
    for key in [
        "RECORD",
        "BURN_IN",
        "PROFILE_STARTUP",
        "PROFILE_RENDER",
        "GRID",
        "SHOW_FPS",
        "SHOW_TOUCHES",
        "RECORD_OUTPUT",
        "RECORD_QUALITY",
        "RECORD_SPEED",
        "PROFILE_STATS",
        "STRICT_MODE",
        "ENABLE_VSYNC",
    ] {
        command.env_remove(key);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Independent unchanged source capture original-output-link/original.json:
    // the source resolves the PNG path before Path.with_suffix('.json').
    assert!(target.with_extension("json").exists());
    assert!(!alias.with_extension("json").exists());
    assert!(alias.is_symlink());
    let states: Value =
        serde_json::from_slice(&fs::read(target.with_extension("json")).unwrap()).unwrap();
    assert_eq!(states[0]["state"]["text"], "after");
}
