//! Original CPython captures of stored Unicode and the later encoding boundary.
#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::ui_forms_check::state_difference;
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

fn compare_originals(fixtures: &[&str]) {
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for fixture in fixtures {
        let captures = JsonValue::parse(fixture).unwrap();
        let JsonView::Array(cases) = captures.view() else {
            panic!("original capture must be a list")
        };
        for case in cases {
            let name = case.get("name").unwrap().to_utf8().unwrap();
            let temp = tempfile::tempdir().unwrap();
            let scene = temp.path().join("scene.json");
            let output = temp.path().join("render.png");
            let scene_bytes = case.get("scene_text").map_or_else(
                || case.get("scene").unwrap().to_json().unwrap(),
                |text| text.to_utf8().unwrap(),
            );
            fs::write(&scene, scene_bytes).unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_ui-forms-reference"));
            command
                .arg(&scene)
                .arg(&output)
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
            let result = command.output().unwrap();
            let success = case.get("exit").unwrap().to_json().unwrap() == "0";
            assert_eq!(
                result.status.success(),
                success,
                "{name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                output.exists(),
                case.get("png").unwrap().to_json().unwrap() == "true",
                "{name} PNG artifact"
            );
            let state = output.with_extension("json");
            assert_eq!(
                state.exists(),
                case.get("state").unwrap().to_json().unwrap() == "true",
                "{name} state artifact"
            );
            if state.exists() {
                let bytes = fs::read(state).unwrap();
                if let Some(expected) = case.get("state_sha256") {
                    assert_eq!(
                        format!("{:x}", Sha256::digest(&bytes)),
                        expected.to_utf8().unwrap(),
                        "{name} exact state bytes"
                    );
                } else {
                    let actual = JsonValue::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
                    assert_eq!(
                        state_difference(&actual, &case.get("states").unwrap()).unwrap(),
                        None,
                        "{name} lossless state"
                    );
                }
            }
            if output.exists() {
                assert_eq!(
                    format!("{:x}", Sha256::digest(fs::read(output).unwrap())),
                    case.get("png_sha256").unwrap().to_utf8().unwrap(),
                    "{name} pixels"
                );
            }
        }
    }
}

#[test]
fn actual_cli_preserves_other_families_raw_text_properties_and_selection_when_available() {
    compare_originals(&[
        include_str!("fixtures/ui-forms/family-codepoints-original.json"),
        include_str!("fixtures/ui-forms/family-properties-original.json"),
        include_str!("fixtures/ui-forms/options-selection-original.json"),
        include_str!("fixtures/ui-forms/family-nul-original.json"),
        include_str!("fixtures/ui-forms/mici-membership-original.json"),
    ]);
}
#[test]
fn actual_cli_preserves_arbitrary_python_values_aliasing_and_character_iterables_when_available() {
    compare_originals(&[
        include_str!("fixtures/ui-forms/mici-values-original.json"),
        include_str!("fixtures/ui-forms/value-titles-original.json"),
        include_str!("fixtures/ui-forms/remaining-python-kinds-original.json"),
        include_str!("fixtures/ui-forms/character-iterables-original.json"),
        include_str!("fixtures/ui-forms/slider-value-depth-original.json"),
    ]);
}

#[test]
fn actual_cli_preserves_list_inputs_setter_timing_and_outer_touch_slots_when_available() {
    compare_originals(&[
        include_str!("fixtures/ui-forms/last-types-original.json"),
        include_str!("fixtures/ui-forms/last-setters-original.json"),
        include_str!("fixtures/ui-forms/last-slots-original.json"),
        include_str!("fixtures/ui-forms/last-keyvalues-original.json"),
        include_str!("fixtures/ui-forms/input-list-neighbors-original.json"),
        include_str!("fixtures/ui-forms/input-list-lazy-original.json"),
        include_str!("fixtures/ui-forms/mici-slider-slots-original.json"),
        include_str!("fixtures/ui-forms/slider-shimmer-width-original.json"),
    ]);
}

#[test]
fn actual_cli_preserves_raw_visibility_lazy_cursor_keys_and_options_slots_when_available() {
    compare_originals(&[
        include_str!("fixtures/ui-forms/last-visible-original.json"),
        include_str!("fixtures/ui-forms/list-visible-codepoints-original.json"),
        include_str!("fixtures/ui-forms/keyboard-keyvalue-neighbors-original.json"),
        include_str!("fixtures/ui-forms/options-slots-original.json"),
        include_str!("fixtures/ui-forms/input-floatcursor-key-original.json"),
    ]);
}

#[test]
fn actual_cli_preserves_json_encoder_boundary_after_exporting_png_when_available() {
    compare_originals(&[include_str!(
        "fixtures/ui-forms/mici-encoder-boundary-original.json"
    )]);
}
