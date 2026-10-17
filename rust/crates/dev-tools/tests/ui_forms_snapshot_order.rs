//! Dictionary insertion order is part of the original checker's failure order.
#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::ui_forms_check::state_difference;
use openpilot_log_format::{JsonValue, JsonView};
use std::{fs, path::Path, process::Command};
#[test]
fn original_mici_snapshot_field_order_keeps_the_first_reported_mismatch_when_available() {
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let scenes: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/scenes.json")).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let scene = temporary.path().join("scene.json");
    let png = temporary.path().join("render.png");
    fs::write(&scene, serde_json::to_vec(&scenes["mici-idle"]).unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ui-forms-reference"));
    command
        .args([scene.as_os_str(), png.as_os_str()])
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
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let original =
        JsonValue::parse(include_str!("fixtures/ui-forms/mici-idle-state.json")).unwrap();
    let native =
        JsonValue::parse(&fs::read_to_string(png.with_extension("json")).unwrap()).unwrap();
    fn order(a: &JsonValue, b: &JsonValue) {
        match (a.view(), b.view()) {
            (JsonView::Object(a), JsonView::Object(b)) => {
                assert_eq!(
                    a.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                    b.iter().map(|(k, _)| *k).collect::<Vec<_>>()
                );
                for ((_, a), (_, b)) in a.iter().zip(b.iter()) {
                    order(a, b);
                }
            }
            (JsonView::Array(a), JsonView::Array(b)) => {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(b.iter()) {
                    order(a, b);
                }
            }
            _ => {}
        }
    }
    order(&original, &native);
    let mut broken: serde_json::Value = serde_json::from_str(&original.to_json().unwrap()).unwrap();
    broken[0]["state"]["text"] = "wrong".into();
    broken[0]["state"]["candidate"] = "wrongcandidate".into();
    let broken = JsonValue::parse(&broken.to_string()).unwrap();
    let expected = state_difference(&original, &broken).unwrap().unwrap();
    assert!(expected.contains("root[0].state.text"), "{expected}");
    assert_eq!(
        state_difference(&native, &broken).unwrap().as_deref(),
        Some(expected.as_str())
    );
}
