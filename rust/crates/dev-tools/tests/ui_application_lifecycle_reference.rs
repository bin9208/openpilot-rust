//! Actual original source reference contracts; no candidate Application oracle.
#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::{
    ui_application_lifecycle_raw,
    ui_application_lifecycle_reference::{run, Options},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ffi::OsString, fs, path::Path, process::Command};
const CASE_ENV: &str = "UI_APPLICATION_REFERENCE_TEST_CASE";
fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-application-lifecycle")
}
fn execute_cases(filename: &str) {
    let Some(raylib) = std::env::var_os("STARTUP_UI_RAYLIB_ROOT") else {
        return;
    };
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY").expect("actual raylib library");
    let probe = std::env::var_os("UI_APPLICATION_FFPROBE").unwrap_or_else(|| "ffprobe".into());
    let work = tempfile::tempdir().unwrap();
    let helper = ui_application_lifecycle_raw::build(work.path(), Path::new(&raylib)).unwrap();
    let captures: Value =
        serde_json::from_slice(&fs::read(fixtures().join(filename)).unwrap()).unwrap();
    for capture in captures.as_array().unwrap() {
        let directory = work.path().join(capture["name"].as_str().unwrap());
        fs::create_dir(&directory).unwrap();
        let output = directory.join("out.png");
        let mode = capture["source_mode"]
            .as_str()
            .or_else(|| capture["mode"].as_str())
            .unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "reference_test_child", "--nocapture"])
            .env(CASE_ENV, mode)
            .env("UI_APPLICATION_REFERENCE_OUTPUT", &output)
            .env("UI_APPLICATION_REFERENCE_RAW", &helper)
            .env("STARTUP_UI_RAYLIB_LIBRARY", &library);
        for name in [
            "GRID",
            "PROFILE_RENDER",
            "PROFILE_STATS",
            "RECORD",
            "RECORD_OUTPUT",
            "RECORD_QUALITY",
            "RECORD_SPEED",
            "RECORD_BITRATE",
            "OFFSCREEN",
            "BURN_IN",
            "PROFILE_STARTUP",
            "ENABLE_VSYNC",
            "SHOW_FPS",
            "SHOW_TOUCHES",
            "STRICT_MODE",
        ] {
            command.env_remove(name);
        }
        for (name, value) in capture["environment"].as_object().unwrap() {
            command.env(
                name,
                if value == "@RECORD@" {
                    directory.join("record.mp4").into_os_string()
                } else {
                    value.as_str().unwrap().into()
                },
            );
        }
        let result = command.output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(capture["exit"].as_i64().unwrap() as i32),
            "{}: {}",
            capture["name"],
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            output.exists(),
            capture["png"].as_bool().unwrap(),
            "{} PNG",
            capture["name"]
        );
        assert_eq!(
            output.with_extension("json").exists(),
            capture["state"].as_bool().unwrap(),
            "{} state",
            capture["name"]
        );
        if capture["state"] == true {
            let expected = if let Some(text) = capture["state_text"].as_str() {
                serde_json::from_str(text).unwrap()
            } else {
                capture["result"].clone()
            };
            let actual: Value =
                serde_json::from_slice(&fs::read(output.with_extension("json")).unwrap()).unwrap();
            assert_eq!(actual, expected, "{} full state", capture["name"]);
        }
        if capture["png"] == true {
            let image = openpilot_dev_tools::image_array::read(&output).unwrap();
            assert_eq!(
                serde_json::to_value(&image.shape).unwrap(),
                capture["shape"]
            );
            let mut digest = Sha256::new();
            for value in image.values {
                digest.update(value.to_le_bytes());
            }
            assert_eq!(
                format!("{:x}", digest.finalize()),
                capture["i64_sha256"].as_str().unwrap(),
                "{} all pixels",
                capture["name"]
            );
        }
        let printed = String::from_utf8_lossy(&result.stdout);
        assert_eq!(
            printed.contains("=== Startup profile ==="),
            capture["startup"] == true
        );
        assert_eq!(
            printed.contains("=== Render loop profile ==="),
            capture["profile"] == true
        );
        if let Some(videos) = capture["videos"].as_array().filter(|v| !v.is_empty()) {
            let paths = if capture["name"] == "record" {
                vec![directory.join("record.mp4")]
            } else {
                fs::read_dir(output.with_extension("videos"))
                    .unwrap()
                    .map(|v| v.unwrap().path())
                    .collect()
            };
            assert_eq!(paths.len(), videos.len());
            for (path, expected) in paths.iter().zip(videos) {
                let result = Command::new(&probe)
                    .args([
                        "-v",
                        "error",
                        "-select_streams",
                        "v:0",
                        "-show_entries",
                        "stream=width,height,nb_frames,r_frame_rate",
                        "-of",
                        "json",
                    ])
                    .arg(path)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let actual: Value = serde_json::from_slice(&result.stdout).unwrap();
                assert_eq!(
                    actual["streams"], expected["streams"],
                    "{} actual native encoder",
                    capture["name"]
                );
            }
        }
    }
}
#[test]
fn reference_test_child() {
    let Some(mode) = std::env::var_os(CASE_ENV) else {
        return;
    };
    let options = Options {
        root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        mode: mode.into_string().unwrap(),
        output: std::env::var_os("UI_APPLICATION_REFERENCE_OUTPUT")
            .unwrap()
            .into(),
        raw_helper: std::env::var_os("UI_APPLICATION_REFERENCE_RAW")
            .unwrap()
            .into(),
        raylib_library: std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
            .unwrap()
            .into(),
    };
    let environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    if let Err(error) = run(&options, &environment, &mut std::io::stdout()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[test]
fn five_original_modes_preserve_real_graphics_ticks_and_native_encoder() {
    execute_cases("source-original.json");
}
#[test]
fn twenty_original_environment_cases_preserve_admission_and_files() {
    execute_cases("environment-original.json");
}

#[test]
fn original_resource_close_order_and_partial_state_are_preserved() {
    use openpilot_dev_tools::ui_application_lifecycle_reference::{close_resources, Resources};
    let cases: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/resource-close-original.json"
    ))
    .unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let mut resources = Resources {
            ready: case["name"] != "not-ready",
            textures: vec![
                ("first".into(), "tex1".into()),
                ("second".into(), "tex2".into()),
            ],
            fonts: vec![
                ("first".into(), "font1".into()),
                ("second".into(), "font2".into()),
            ],
            target: Some("target".into()),
            shader: Some("shader".into()),
        };
        let mut calls = vec![];
        let name = case["name"].as_str().unwrap();
        let result = close_resources(&mut resources, &mut |kind, identity| {
            calls.push(if let Some(identity) = identity {
                serde_json::json!([kind, identity])
            } else {
                serde_json::json!([kind])
            });
            if name == format!("{kind}-error") {
                Err(format!("controlled {kind}").into())
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_ok(), case["error"].is_null(), "{name}");
        if let Some(message) = case["error"]["message"].as_str() {
            assert_eq!(result.err().unwrap().to_string(), message);
        }
        assert_eq!(
            serde_json::to_value(calls).unwrap(),
            case["calls"],
            "{name}"
        );
        let textures = resources
            .textures
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect::<serde_json::Map<_, _>>();
        let fonts = resources
            .fonts
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(
            serde_json::json!({"textures":textures,"fonts":fonts,"target":resources.target,"shader":resources.shader}),
            case["remaining"],
            "{name} ownership"
        );
    }
}

#[test]
fn original_touch_overlay_commands_colors_history_and_expiry() {
    use openpilot_dev_tools::{
        seal::Result,
        ui_application_lifecycle_loop::{MouseEvent, Position},
        ui_application_lifecycle_reference::draw_touch_points,
        ui_forms_input::Canvas,
    };
    struct Calls(Vec<Value>);
    impl Canvas for Calls {
        fn call(&mut self, command: &str) -> Result<()> {
            let fields = command.split_whitespace().collect::<Vec<_>>();
            assert_eq!(fields[0], "circle");
            let color = fields[4].parse::<u32>()?.to_le_bytes();
            self.0.push(serde_json::json!([
                fields[1].parse::<i64>()?,
                fields[2].parse::<i64>()?,
                fields[3].parse::<i64>()?,
                color,
            ]));
            Ok(())
        }
        fn measure(&mut self, _: &str, _: f64) -> Result<f64> {
            panic!("touch overlay does not measure text")
        }
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/touch-draw-original.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut history = case["history"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v[0].as_f64().unwrap(),
                    v[1].as_f64().unwrap(),
                    v[2].as_f64().unwrap(),
                )
            })
            .collect();
        let now = case["now"].as_f64().unwrap();
        let events = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(slot, v)| MouseEvent {
                pos: Position {
                    x: v[0].as_f64().unwrap(),
                    y: v[1].as_f64().unwrap(),
                },
                slot,
                pressed: v[2].as_bool().unwrap(),
                released: false,
                down: false,
                time: now,
            })
            .collect::<Vec<_>>();
        let mut calls = Calls(vec![]);
        draw_touch_points(&mut calls, &mut history, &events, now).unwrap();
        assert_eq!(
            serde_json::json!(calls.0),
            case["calls"],
            "{} commands",
            case["name"]
        );
        let expected = case["remaining"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v[0].as_f64().unwrap(),
                    v[1].as_f64().unwrap(),
                    v[2].as_f64().unwrap(),
                )
            })
            .collect::<std::collections::VecDeque<_>>();
        assert_eq!(history, expected, "{} history", case["name"]);
    }
}

#[test]
fn transitive_sources_fonts_absence_and_dependency_records_reject_drift() {
    use openpilot_dev_tools::ui_application_lifecycle_reference::validate_source;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let directory = tempfile::tempdir().unwrap();
    let copy = directory.path();
    let closure: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/source-closure.json"
    ))
    .unwrap();
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/provenance.json"
    ))
    .unwrap();
    let mut paths = closure["sources"]
        .as_object()
        .unwrap()
        .keys()
        .chain(original["sources"].as_object().unwrap().keys())
        .map(String::as_str)
        .collect::<Vec<_>>();
    paths.push("uv.lock");
    paths.sort_unstable();
    paths.dedup();
    for relative in paths {
        let from = root.join(relative);
        if !from.exists()
            && matches!(
                relative,
                "rust/tools/check_ui_application.py" | "rust/tools/ui_qa/application_source.py"
            )
        {
            continue;
        }
        let destination = copy.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(from, destination).unwrap();
    }
    validate_source(copy).unwrap();
    for name in closure["sources"].as_object().unwrap().keys() {
        let path = copy.join(name);
        let saved = fs::read(&path).unwrap();
        fs::write(&path, b"changed independently pinned source").unwrap();
        assert!(validate_source(copy).is_err(), "changed {name} admitted");
        fs::remove_file(&path).unwrap();
        assert!(validate_source(copy).is_err(), "missing {name} admitted");
        fs::write(&path, saved).unwrap();
    }
    for name in closure["absent"].as_array().unwrap() {
        let path = copy.join(name.as_str().unwrap());
        fs::write(&path, b"new source font shadows original fallback").unwrap();
        assert!(validate_source(copy).is_err(), "new font {} admitted", name);
        fs::remove_file(path).unwrap();
    }
    let lock = copy.join("uv.lock");
    let saved = fs::read_to_string(&lock).unwrap();
    for package in closure["dependencies"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        let version = package["version"].as_str().unwrap();
        let marker = format!("\nname = \"{name}\"\nversion = \"{version}\"");
        assert!(saved.contains(&marker));
        fs::write(
            &lock,
            saved.replacen(
                &marker,
                &format!("\nname = \"{name}\"\nversion = \"0.0.0\""),
                1,
            ),
        )
        .unwrap();
        assert!(
            validate_source(copy).is_err(),
            "{name} version drift admitted"
        );
        fs::write(&lock, &saved).unwrap();
    }
    for name in [
        "params_pyx",
        "params_pyx.py",
        "params_pyx.pyc",
        "params_pyx.cpython-312-x86_64-linux-gnu.so",
    ] {
        let path = copy.join("openpilot/common").join(name);
        if name == "params_pyx" {
            fs::create_dir(&path).unwrap();
        } else {
            fs::write(&path, b"new Params import provider").unwrap();
        }
        assert!(
            validate_source(copy).is_err(),
            "new {name} import provider admitted"
        );
        if name == "params_pyx" {
            fs::remove_dir(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
    for retired in [
        "rust/tools/check_ui_application.py",
        "rust/tools/ui_qa/application_source.py",
    ] {
        let path = copy.join(retired);
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        validate_source(copy).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"changed reintroduced retired source").unwrap();
        assert!(validate_source(copy).is_err(), "changed {retired} admitted");
        fs::remove_file(path).unwrap();
    }
}
