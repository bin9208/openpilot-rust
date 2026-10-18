//! Real unchanged-source PNG/state fixtures, evaluated through the native ABI.
#![cfg(feature = "image-array-native")]
#[allow(dead_code)]
#[path = "support/ui_forms_confirm_draw.rs"]
mod confirm_draw;
#[allow(dead_code)]
#[path = "support/ui_reference_draw.rs"]
mod reference_draw;
use openpilot_dev_tools::{
    ui_forms_input::Canvas,
    ui_render_reference::{render, validate_source, Raw},
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[test]
fn actual_original_default_and_generic_render_contract_when_available() {
    let Some(helper) = std::env::var_os("UI_RENDER_RAW_HELPER") else {
        return;
    };
    let library =
        std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY").expect("native render proof requires raylib");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for fixture in [
        include_str!("fixtures/ui-render/original.json"),
        include_str!("fixtures/ui-render/generic-original.json"),
    ] {
        let cases: Value = serde_json::from_str(fixture).unwrap();
        for case in cases.as_array().unwrap() {
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("render.png");
            let actual = render(
                &root,
                &reference_draw::scene_bytes(fixture, case).unwrap(),
                &output,
                Path::new(&helper),
                Path::new(&library),
            );
            assert_eq!(
                actual.is_ok(),
                case["exit"] == 0,
                "{} {:?}",
                case["name"],
                actual.as_ref().err()
            );
            assert_eq!(output.exists(), case["png"] == true, "{} PNG", case["name"]);
            if let Ok(Some(states)) = actual {
                let actual = openpilot_log_format::JsonValue::parse(&states.to_string()).unwrap();
                let expected =
                    openpilot_log_format::JsonValue::parse(&case["states"].to_string()).unwrap();
                assert!(
                    openpilot_dev_tools::ui_render_reference::states_equal(&actual, &expected),
                    "{} state",
                    case["name"]
                );
                reference_draw::assert_png(
                    fixture,
                    &reference_draw::from_serde_case(fixture, case).unwrap(),
                    &root,
                    Path::new(&helper),
                    &std::process::Command::new("unused"),
                    temp.path(),
                    &output,
                );
            }
        }
    }
}
#[test]
fn source_font_absence_dependency_and_params_import_guards() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/ui-render/provenance.json")).unwrap();
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
        }
    }
    validate_source(temp.path()).unwrap();
    for name in pins["sources"].as_object().unwrap().keys() {
        let path = temp.path().join(name);
        if !path.exists() {
            assert!(matches!(
                name.as_str(),
                "rust/tools/check_ui_render.py" | "rust/tools/ui_framework_source_render.py"
            ));
            fs::write(&path, b"reintroduced changed original").unwrap();
            assert!(validate_source(temp.path()).is_err(), "reintroduced {name}");
            fs::remove_file(&path).unwrap();
            continue;
        }
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, b"changed source").unwrap();
        assert!(validate_source(temp.path()).is_err(), "changed {name}");
        fs::remove_file(&path).unwrap();
        let retired = matches!(
            name.as_str(),
            "rust/tools/check_ui_render.py" | "rust/tools/ui_framework_source_render.py"
        );
        assert_eq!(
            validate_source(temp.path()).is_ok(),
            retired,
            "absent {name}"
        );
        fs::write(path, bytes).unwrap();
    }
    for name in [
        "rust/tools/check_ui_render.py",
        "rust/tools/ui_framework_source_render.py",
    ] {
        let path = temp.path().join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    validate_source(temp.path()).unwrap();
    for name in [
        "rust/tools/check_ui_render.py",
        "rust/tools/ui_framework_source_render.py",
    ] {
        let path = temp.path().join(name);
        fs::write(&path, b"reintroduced changed original").unwrap();
        assert!(validate_source(temp.path()).is_err(), "reintroduced {name}");
        fs::remove_file(&path).unwrap();
    }
    for name in pins["absent"].as_array().unwrap() {
        let path = temp.path().join(name.as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"new font namespace").unwrap();
        assert!(validate_source(temp.path()).is_err(), "added {name}");
        fs::remove_file(path).unwrap();
    }
    let lock = temp.path().join("uv.lock");
    let bytes = fs::read_to_string(&lock).unwrap();
    for name in ["numpy", "pillow", "comma-deps-raylib", "cffi", "pycparser"] {
        fs::write(
            &lock,
            bytes.replace(
                &format!("name = \"{name}\""),
                &format!("name = \"changed-{name}\""),
            ),
        )
        .unwrap();
        assert!(validate_source(temp.path()).is_err(), "dependency {name}");
        fs::write(&lock, &bytes).unwrap();
    }
    for name in [
        "params_pyx",
        "params_pyx.py",
        "params_pyx.cpython-312-x86_64-linux-gnu.so",
    ] {
        let path = temp.path().join("openpilot/common").join(name);
        fs::write(&path, b"new Params import").unwrap();
        assert!(validate_source(temp.path()).is_err(), "namespace {name}");
        fs::remove_file(path).unwrap();
    }
}
#[cfg(unix)]
#[test]
fn raw_owned_child_cleanup_at_malformed_reply_eof_and_command_errors() {
    use std::os::unix::fs::PermissionsExt;
    for (phase, reply) in [
        ("open", "bad"),
        ("open", "ERR f"),
        ("open", "EOF"),
        ("command", "bad"),
        ("command", "ERR ff"),
        ("command", "EOF"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let helper = temp.path().join("helper");
        let pid = temp.path().join("pid");
        let first = if phase == "command" {
            "printf 'OK\\n'; IFS= read -r command\n"
        } else {
            ""
        };
        let last = if reply == "EOF" {
            "exit 0\n".into()
        } else {
            format!("printf '%s\\n' '{reply}'\nwhile IFS= read -r command; do :; done\n")
        };
        fs::write(
            &helper,
            format!("#!/bin/sh\nprintf '%s' \"$$\" > \"$1\"\n{first}{last}"),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        if phase == "open" {
            assert!(Raw::open(&helper, &pid).is_err());
        } else {
            let mut raw = Raw::open(&helper, &pid).unwrap();
            assert!(raw.call("flags 32").is_err());
            drop(raw);
        }
        let child = fs::read_to_string(pid).unwrap();
        assert!(!Path::new("/proc").join(child).exists(), "{phase}/{reply}");
    }
}

#[test]
fn original_raw_text_ignored_fields_and_toggle_state_when_available() {
    use openpilot_log_format::{JsonValue, JsonView};
    let Some(helper) = std::env::var_os("UI_RENDER_RAW_HELPER") else {
        return;
    };
    let library =
        std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY").expect("raw render gate needs raylib");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for fixture in [
        include_str!("fixtures/ui-render/admission-original.json"),
        include_str!("fixtures/ui-render/constructor-original.json"),
        include_str!("fixtures/ui-render/constructor-numeric-original.json"),
        include_str!("fixtures/ui-render/label-constructor-original.json"),
        include_str!("fixtures/ui-render/events-original.json"),
        include_str!("fixtures/ui-render/numeric-events-original.json"),
        include_str!("fixtures/ui-render/whitespace-original.json"),
        include_str!("fixtures/ui-render/nul-original.json"),
        include_str!("fixtures/ui-render/multiclick-original.json"),
        include_str!("fixtures/ui-render/multiclick-values-original.json"),
    ] {
        let captured = JsonValue::parse(fixture).unwrap();
        let JsonView::Array(cases) = captured.view() else {
            panic!("original cases must array");
        };
        for case in cases {
            let name = case.get("name").unwrap().to_utf8().unwrap();
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("render.png");
            let result = openpilot_dev_tools::ui_render_reference::render_lossless(
                &root,
                case.get("scene_text")
                    .unwrap()
                    .to_utf8()
                    .unwrap()
                    .as_bytes(),
                &output,
                Path::new(&helper),
                Path::new(&library),
            );
            assert_eq!(
                result.is_ok(),
                case.get("exit").unwrap().to_json().unwrap() == "0",
                "{name}: {:?}",
                result.as_ref().err()
            );
            assert_eq!(
                output.exists(),
                case.get("png").unwrap().to_json().unwrap() == "true",
                "{name} PNG"
            );
            if let Ok(Some(value)) = result {
                let expected =
                    JsonValue::parse(&case.get("state_text").unwrap().to_utf8().unwrap()).unwrap();
                assert!(
                    openpilot_dev_tools::ui_render_reference::states_equal(&value, &expected),
                    "{name} raw state"
                );
                reference_draw::assert_png(
                    fixture,
                    &case,
                    &root,
                    Path::new(&helper),
                    &std::process::Command::new("unused"),
                    temp.path(),
                    &output,
                );
            }
        }
    }
}

#[test]
fn actual_cli_preserves_original_environment_artifacts_and_recorder_when_available() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let Some(helper) = std::env::var_os("UI_RENDER_RAW_HELPER") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixture = include_str!("fixtures/ui-render/environment-original.json");
    let captures: Vec<Value> = serde_json::from_str(fixture).unwrap();
    for case in &captures {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("ffmpeg");
        // Owned shell peer records exact argv and raw input; production does
        // not use this test peer or an interpreter.
        std::fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$RENDER_FFMPEG_ARGS\"\n/bin/cat > \"$RENDER_FFMPEG_BYTES\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let scene = temp.path().join("scene.json");
        std::fs::write(&scene, reference_draw::scene_bytes(fixture, case).unwrap()).unwrap();
        let output = temp.path().join("output.png");
        let record = temp.path().join("record.mp4");
        let arguments = temp.path().join("args");
        let bytes = temp.path().join("bytes");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ui-render-reference"));
        command
            .args([&scene, &output])
            .env("OPENPILOT_RUST_ROOT", &root)
            .env("UI_RENDER_RAW_HELPER", &helper)
            .env("PATH", temp.path())
            .env("RECORD_OUTPUT", &record)
            .env("RENDER_FFMPEG_ARGS", &arguments)
            .env("RENDER_FFMPEG_BYTES", &bytes);
        for key in [
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
            "RECORD_QUALITY",
            "RECORD_SPEED",
            "RECORD_BITRATE",
        ] {
            command.env_remove(key);
        }
        for (key, value) in case["environment"].as_object().into_iter().flatten() {
            command.env(key, value.as_str().unwrap());
        }
        let observed = command.output().unwrap();
        assert_eq!(
            observed.status.code(),
            case["exit"].as_i64().map(|v| v as i32),
            "{}: {}",
            case["name"],
            String::from_utf8_lossy(&observed.stderr)
        );
        assert_eq!(output.exists(), case["png"] == true, "{} PNG", case["name"]);
        assert_eq!(
            output.with_extension("json").exists(),
            case["state"] == true,
            "{} states",
            case["name"]
        );
        let stdout = String::from_utf8_lossy(&observed.stdout);
        assert_eq!(
            stdout.contains("=== Startup profile ==="),
            case["startup"] == true,
            "{} startup",
            case["name"]
        );
        assert_eq!(
            stdout.contains("=== Render loop profile ==="),
            case["render_profile"] == true,
            "{} render",
            case["name"]
        );
        let mut expected_frames = None;
        if output.exists() {
            expected_frames = Some(
                reference_draw::assert_png(
                    fixture,
                    &reference_draw::from_serde_case(fixture, case).unwrap(),
                    &root,
                    Path::new(&helper),
                    &command,
                    temp.path(),
                    &output,
                )
                .frames,
            );
            let states: Value =
                serde_json::from_slice(&std::fs::read(output.with_extension("json")).unwrap())
                    .unwrap();
            let actual = openpilot_log_format::JsonValue::parse(&states.to_string()).unwrap();
            let expected =
                openpilot_log_format::JsonValue::parse(&case["states"].to_string()).unwrap();
            assert!(
                openpilot_dev_tools::ui_render_reference::states_equal(&actual, &expected),
                "{} state",
                case["name"]
            );
        }
        if let Some(recorder) = case.get("recorder") {
            let actual = std::fs::read_to_string(arguments)
                .unwrap()
                .replace(record.to_str().unwrap(), "@RECORD_OUTPUT@");
            let expected = recorder["argv"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            assert_eq!(actual, expected, "{} recorder argv", case["name"]);
            let frames = std::fs::read(bytes).unwrap();
            assert_eq!(
                frames.len() as u64,
                recorder["bytes"].as_u64().unwrap(),
                "{} recorder frames",
                case["name"]
            );
            // PROFILE_STARTUP exits before entering the draw generator:
            // its original recorder exists but receives no pixels or PNG.
            let expected = if case["png"] == false {
                assert_eq!(
                    recorder["bytes"], 0,
                    "{} recording without drawing",
                    case["name"]
                );
                Vec::new()
            } else {
                expected_frames
                    .as_ref()
                    .expect("original recording draw frames")
                    .iter()
                    .flat_map(|path| std::fs::read(path).unwrap())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                frames, expected,
                "{} exact original recorded pixels",
                case["name"]
            );
        } else {
            assert!(!arguments.exists(), "{} unexpected recorder", case["name"]);
        }
    }
}
