//! Independent original-process, state and pixel contracts for network screens.
#![cfg(feature = "image-array-native")]
#[allow(dead_code)]
#[path = "support/ui_forms_confirm_draw.rs"]
mod confirm_draw;
#[allow(dead_code)]
#[path = "support/ui_reference_draw.rs"]
mod reference_draw;
#[path = "ui_network_reference/source_binding.rs"]
mod source_binding;
use openpilot_dev_tools::{
    ui_forms_input::Canvas,
    ui_network_reference::{render, validate_source},
};
use openpilot_log_format::JsonValue;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn paths() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let helper = std::env::var_os("UI_NETWORK_RAW_HELPER")
        .or_else(|| std::env::var_os("UI_FORMS_RAW_HELPER"))?;
    let library =
        std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY").expect("network raw proof requires raylib");
    Some((
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        helper.into(),
        library.into(),
    ))
}
#[test]
fn actual_original_scenes_branching_and_generic_inputs() {
    let Some((root, helper, library)) = paths() else {
        return;
    };
    for fixture in [
        include_str!("fixtures/ui-network/original.json"),
        include_str!("fixtures/ui-network/policy-original.json"),
        include_str!("fixtures/ui-network/generic-original.json"),
        include_str!("fixtures/ui-network/interactions-original.json"),
        include_str!("fixtures/ui-network/lazy-original.json"),
        include_str!("fixtures/ui-network/callbacks-original.json"),
        include_str!("fixtures/ui-network/membership-original.json"),
    ] {
        let cases: Value = serde_json::from_str(fixture).unwrap();
        for case in cases.as_array().unwrap() {
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("network.png");
            let scene = reference_draw::scene_bytes(fixture, case).unwrap();
            let actual = render(&root, &scene, &output, &helper, &library);
            assert_eq!(
                actual.is_ok(),
                case["exit"] == 0,
                "{} {:?}",
                case["name"],
                actual.as_ref().err()
            );
            assert_eq!(
                output.exists(),
                case["png"] == true,
                "{} PNG existence",
                case["name"]
            );
            if let Ok(Some(state)) = actual {
                let expected = if let Some(s) = case["state_text"].as_str() {
                    JsonValue::parse(s).unwrap()
                } else {
                    JsonValue::parse(&case["states"].to_string()).unwrap()
                };
                assert!(
                    openpilot_dev_tools::ui_network_reference::states_equal(&state, &expected),
                    "{} state: {} != {}",
                    case["name"],
                    state.to_json().unwrap(),
                    expected.to_json().unwrap()
                );
            }
            if output.exists() {
                let replay = reference_draw::assert_png(
                    fixture,
                    &reference_draw::from_serde_case(fixture, case).unwrap(),
                    &root,
                    Path::new(&helper),
                    &std::process::Command::new("unused"),
                    temp.path(),
                    &output,
                );
                let image = openpilot_dev_tools::image_array::read(&output).unwrap();
                assert_eq!(
                    image.shape,
                    case["shape"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as usize)
                        .collect::<Vec<_>>()
                );
                let expected_image = openpilot_dev_tools::image_array::read(&replay.path).unwrap();
                assert_eq!(
                    image.values, expected_image.values,
                    "{} exact i64 samples",
                    case["name"]
                );
            }
        }
    }
}
#[test]
fn actual_original_manual_loop_environment() {
    let Some((root, helper, library)) = paths() else {
        return;
    };
    let executable = std::env::var_os("CARGO_BIN_EXE_ui-network-reference")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_ui-network-reference")));
    let fixture = include_str!("fixtures/ui-network/environment-original.json");
    let cases: Value = serde_json::from_str(fixture).unwrap();
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let scene = temp.path().join("scene.json");
        let output = temp.path().join("out.png");
        fs::write(&scene, case["scene_text"].as_str().unwrap()).unwrap();
        let mut command = std::process::Command::new(&executable);
        command
            .args([&scene, &output])
            .current_dir(&root)
            .env("UI_NETWORK_RAW_HELPER", &helper)
            .env("STARTUP_UI_RAYLIB_LIBRARY", &library);
        for name in [
            "RECORD",
            "BURN_IN",
            "PROFILE_STARTUP",
            "PROFILE_RENDER",
            "PROFILE_STATS",
            "GRID",
            "SHOW_FPS",
            "SHOW_TOUCHES",
            "STRICT_MODE",
            "ENABLE_VSYNC",
        ] {
            command.env_remove(name);
        }
        for (name, value) in case["environment"].as_object().unwrap() {
            command.env(name, value.as_str().unwrap());
        }
        let result = command.output().unwrap();
        assert_eq!(
            result.status.success(),
            case["exit"] == 0,
            "{} {}",
            case["name"],
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(output.exists(), case["png"] == true, "{} PNG", case["name"]);
        assert_eq!(
            output.with_extension("json").exists(),
            case["state"] == true,
            "{} state",
            case["name"]
        );
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).contains("Startup profile"),
            case["startup"] == true
        );
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).contains("Render loop profile"),
            case["render_profile"] == true
        );
        if output.exists() {
            reference_draw::assert_png(
                fixture,
                &reference_draw::from_serde_case(fixture, case).unwrap(),
                &root,
                Path::new(&helper),
                &command,
                temp.path(),
                &output,
            );
        }
    }
}
#[test]
fn each_source_asset_dependency_and_absence_pin_is_required() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/ui-network/provenance.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for name in pins["sources"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("uv.lock"))
    {
        let path = temp.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if root.join(name).exists() {
            fs::copy(root.join(name), path).unwrap();
        } else {
            assert!(
                matches!(
                    name,
                    "rust/tools/check_ui_network.py" | "rust/tools/ui_network_source.py"
                ),
                "live source missing: {name}"
            );
        }
    }
    validate_source(temp.path()).unwrap();
    for name in pins["sources"].as_object().unwrap().keys() {
        let path = temp.path().join(name);
        if !path.exists() {
            continue;
        } // The two retired originals may already be absent.
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"changed").unwrap();
        assert!(validate_source(temp.path()).is_err(), "{name} change");
        fs::remove_file(&path).unwrap();
        let retired = matches!(
            name.as_str(),
            "rust/tools/check_ui_network.py" | "rust/tools/ui_network_source.py"
        );
        assert_eq!(
            validate_source(temp.path()).is_ok(),
            retired,
            "{name} absent"
        );
        fs::write(path, original).unwrap();
    }
    let originals = [
        "rust/tools/check_ui_network.py",
        "rust/tools/ui_network_source.py",
    ];
    for name in originals {
        let path = temp.path().join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    validate_source(temp.path()).unwrap();
    openpilot_dev_tools::ui_network_check::validate_source(temp.path()).unwrap();
    for name in originals {
        let path = temp.path().join(name);
        fs::write(&path, b"changed reintroduced original").unwrap();
        assert!(validate_source(temp.path()).is_err());
        assert!(openpilot_dev_tools::ui_network_check::validate_source(temp.path()).is_err());
        fs::remove_file(path).unwrap();
    }
    for name in pins["absent"].as_array().unwrap() {
        let path = temp.path().join(name.as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"new-font").unwrap();
        assert!(validate_source(temp.path()).is_err());
        fs::remove_file(path).unwrap();
    }
    let lock = temp.path().join("uv.lock");
    let original = fs::read_to_string(&lock).unwrap();
    fs::write(
        lock,
        original.replacen("\nname = \"numpy\"\n", "\nname = \"numpy-changed\"\n", 1),
    )
    .unwrap();
    assert!(validate_source(temp.path()).is_err());
}
#[test]
fn malformed_raw_reply_and_eof_clean_up_owned_helper() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let helper = temp.path().join("helper");
    for content in [
        "#!/bin/sh\necho bad\n",
        "#!/bin/sh\necho OK\nread line\nexit 0\n",
    ] {
        fs::write(&helper, content).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        if let Ok(mut raw) =
            openpilot_dev_tools::ui_forms_reference::Raw::open(&helper, Path::new("unused"))
        {
            assert!(raw.call("begin").is_err());
        }
    }
}
