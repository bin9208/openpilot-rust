//! Independently captured InputBox source state and raylib drawing calls.
use openpilot_dev_tools::{seal::Result, ui_forms_input::Canvas, ui_forms_reference::render_input};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
struct OriginalMeasurements {
    measurements: BTreeMap<(String, u64), f64>,
    events: Vec<Value>,
}
fn text(s: &str) -> String {
    if s == "-" {
        return String::new();
    }
    String::from_utf8(
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect(),
    )
    .unwrap()
}
impl Canvas for OriginalMeasurements {
    fn measure(&mut self, text: &str, size: f64) -> Result<f64> {
        self.measurements
            .get(&(text.into(), size.to_bits()))
            .copied()
            .ok_or_else(|| format!("uncaptured original measurement {text:?}/{size}").into())
    }
    fn call(&mut self, line: &str) -> Result<()> {
        let p = line.split(' ').collect::<Vec<_>>();
        let f = |at: usize| p[at].parse::<f64>().unwrap();
        let color = |at: usize| p[at].parse::<u32>().unwrap().to_le_bytes();
        let row = match p[0] {
            "flags" | "window" | "font" => return Ok(()),
            "begin" => json!(["begin_drawing"]),
            "end" => json!(["end_drawing"]),
            "clear" => json!(["clear_background"]),
            "export" => {
                self.events.push(json!(["rl_draw_render_batch_active"]));
                json!(["export_image"])
            }
            "rect" => json!(["draw_rectangle_rec", f(1), f(2), f(3), f(4), color(5)]),
            "outline" => json!(["draw_rectangle_lines", f(1), f(2), f(3), f(4), color(5)]),
            "line" => json!(["draw_line", f(1), f(2), f(3), f(4), color(5)]),
            "text" => json!(["draw_text_ex", text(p[1]), f(2), f(3), f(4), 0, color(5)]),
            "clip" => json!(["begin_scissor_mode", f(1), f(2), f(3), f(4)]),
            "unclip" => json!(["end_scissor_mode"]),
            _ => panic!("unexpected forms raw command {line}"),
        };
        self.events.push(row);
        Ok(())
    }
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (k, v) in a {
                compare(v, &b[k]);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= 2e-5,
            "{a}!={b}"
        ),
        _ => assert_eq!(a, b),
    }
}
#[test]
fn independent_input_family_matches_every_original_frame_and_draw() {
    let scenes: Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/scenes.json")).unwrap();
    let traces: Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/input-traces.json")).unwrap();
    for name in ["debug-input", "input", "password"] {
        let trace = traces[name].as_array().unwrap();
        let measurements = trace
            .iter()
            .filter(|r| r[0] == "measure_text_ex")
            .map(|r| {
                (
                    (
                        r[1].as_str().unwrap().into(),
                        r[2].as_f64().unwrap().to_bits(),
                    ),
                    r[4].as_f64().unwrap(),
                )
            })
            .collect();
        let mut api = OriginalMeasurements {
            measurements,
            events: vec![],
        };
        let observed = render_input(
            Path::new("/source"),
            &serde_json::to_vec(&scenes[name]).unwrap(),
            Path::new("/unused.png"),
            &mut api,
        )
        .unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ui-forms")
            .join(format!("{name}-state.json"));
        let expected: Value = serde_json::from_slice(&std::fs::read(fixture).unwrap()).unwrap();
        compare(&observed, &expected);
        // Source captures before generator.close: its last frame is not presented.
        assert_eq!(api.events.pop(), Some(json!(["end_drawing"])));
        let expected = Value::Array(
            trace
                .iter()
                .filter(|r| r[0] != "measure_text_ex")
                .cloned()
                .collect(),
        );
        compare(&Value::Array(api.events), &expected);
    }
}

#[test]
fn raw_native_input_family_against_independent_adversarial_sources_when_available() {
    use openpilot_dev_tools::ui_forms_reference::Raw;
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .expect("set raylib library for native forms proof");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let captures = [
        include_str!("fixtures/ui-forms/reference-default-original.json"),
        include_str!("fixtures/ui-forms/input-adversarial-original.json"),
        include_str!("fixtures/ui-forms/html-adversarial-original.json"),
        include_str!("fixtures/ui-forms/surface-original.json"),
        include_str!("fixtures/ui-forms/emoji-pending-original.json"),
        include_str!("fixtures/ui-forms/generic-original.json"),
        include_str!("fixtures/ui-forms/debug-original.json"),
    ];
    for capture in captures {
        let cases: Value = serde_json::from_str(capture).unwrap();
        for case in cases.as_array().unwrap() {
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("input.png");
            let mut raw = Raw::open(Path::new(&helper), Path::new(&library)).unwrap();
            let actual = render_input(
                &root,
                &serde_json::to_vec(&case["scene"]).unwrap(),
                &output,
                &mut raw,
            )
            .unwrap();
            raw.close().unwrap();
            compare(&actual, &case["states"]);
            use sha2::{Digest, Sha256};
            assert_eq!(
                format!("{:x}", Sha256::digest(std::fs::read(&output).unwrap())),
                case["png_sha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn actual_cli_preserves_original_environment_artifacts_and_recorder_when_available() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let captures = [
        include_str!("fixtures/ui-forms/environment-original.json"),
        include_str!("fixtures/ui-forms/error-original.json"),
        include_str!("fixtures/ui-forms/cursor-original.json"),
        include_str!("fixtures/ui-forms/mouse-slots-original.json"),
    ]
    .iter()
    .flat_map(|v| {
        serde_json::from_str::<Value>(v)
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    })
    .collect::<Vec<_>>();
    for case in &captures {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("ffmpeg");
        // Owned shell peer records exact argv and raw input; production does
        // not use this test peer or an interpreter.
        std::fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$FORMS_FFMPEG_ARGS\"\n/bin/cat > \"$FORMS_FFMPEG_BYTES\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let scene = temp.path().join("scene.json");
        std::fs::write(&scene, serde_json::to_vec(&case["scene"]).unwrap()).unwrap();
        let output = temp.path().join("output.png");
        let record = temp.path().join("record.mp4");
        let arguments = temp.path().join("args");
        let bytes = temp.path().join("bytes");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ui-forms-reference"));
        command
            .args([&scene, &output])
            .env("OPENPILOT_RUST_ROOT", &root)
            .env("UI_FORMS_RAW_HELPER", &helper)
            .env("PATH", temp.path())
            .env("RECORD_OUTPUT", &record)
            .env("FORMS_FFMPEG_ARGS", &arguments)
            .env("FORMS_FFMPEG_BYTES", &bytes);
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
        if output.exists() {
            use sha2::{Digest, Sha256};
            assert_eq!(
                format!("{:x}", Sha256::digest(std::fs::read(&output).unwrap())),
                case["png_sha256"].as_str().unwrap(),
                "{} PNG",
                case["name"]
            );
            let states: Value =
                serde_json::from_slice(&std::fs::read(output.with_extension("json")).unwrap())
                    .unwrap();
            compare(&states, &case["states"]);
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
            use sha2::{Digest, Sha256};
            assert_eq!(
                format!("{:x}", Sha256::digest(&frames)),
                recorder["sha256"].as_str().unwrap(),
                "{} recorded pixels",
                case["name"]
            );
        } else {
            assert!(!arguments.exists(), "{} unexpected recorder", case["name"]);
        }
    }
}

#[test]
fn original_runtime_font_namespace_and_dependency_drift_fail_closed() {
    use openpilot_dev_tools::ui_forms_reference::validate_source;
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/input-provenance.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for name in pins["sources"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("uv.lock"))
    {
        let destination = temp.path().join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        if root.join(name).exists() {
            fs::copy(root.join(name), destination).unwrap();
        }
    }
    validate_source(temp.path()).unwrap();
    for name in pins["sources"].as_object().unwrap().keys() {
        let path = temp.path().join(name);
        let original = fs::read(&path).ok();
        fs::write(&path, b"changed original").unwrap();
        assert!(validate_source(temp.path()).is_err(), "changed {name}");
        fs::remove_file(&path).unwrap();
        let retired = matches!(
            name.as_str(),
            "rust/tools/check_ui_forms.py" | "rust/tools/ui_forms_source.py"
        );
        assert_eq!(
            validate_source(temp.path()).is_ok(),
            retired,
            "absent {name}"
        );
        if let Some(original) = original {
            fs::write(&path, original).unwrap();
        }
    }
    let originals = [
        "rust/tools/check_ui_forms.py",
        "rust/tools/ui_forms_source.py",
    ];
    for name in originals {
        let path = temp.path().join(name);
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }
    validate_source(temp.path()).unwrap();
    for name in originals {
        std::fs::write(temp.path().join(name), b"modified reintroduced original").unwrap();
        assert!(validate_source(temp.path()).is_err(), "reintroduced {name}");
        std::fs::remove_file(temp.path().join(name)).unwrap();
    }
    for name in pins["absent"].as_array().unwrap() {
        let path = temp.path().join(name.as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"new namespace or font").unwrap();
        assert!(validate_source(temp.path()).is_err(), "added {name}");
        fs::remove_file(path).unwrap();
    }
    let lock = temp.path().join("uv.lock");
    let original = fs::read_to_string(&lock).unwrap();
    for package in ["numpy", "pillow", "comma-deps-raylib", "cffi", "pycparser"] {
        fs::write(
            &lock,
            original.replace(
                &format!("name = \"{package}\""),
                &format!("name = \"changed-{package}\""),
            ),
        )
        .unwrap();
        assert!(
            validate_source(temp.path()).is_err(),
            "dependency {package}"
        );
        fs::write(&lock, &original).unwrap();
    }
    fs::remove_file(&lock).unwrap();
    assert!(validate_source(temp.path()).is_err());
    fs::write(&lock, original).unwrap();
    for name in [
        "params_pyx.py",
        "params_pyx.cpython-312-x86_64-linux-gnu.so",
        "params_pyx",
    ] {
        let path = temp.path().join("openpilot/common").join(name);
        fs::write(&path, b"new importable Params implementation").unwrap();
        assert!(validate_source(temp.path()).is_err(), "{name}");
        fs::remove_file(path).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn owned_raw_child_is_reaped_on_malformed_reply_eof_and_command_failure() {
    use openpilot_dev_tools::ui_forms_reference::Raw;
    use std::{fs, os::unix::fs::PermissionsExt};
    for (phase, reply) in [
        ("open", "invalid"),
        ("open", "ERR f"),
        ("open", "EOF"),
        ("command", "invalid"),
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
            assert!(Raw::open(&helper, &pid).is_err(), "{phase}/{reply}");
        } else {
            let mut raw = Raw::open(&helper, &pid).unwrap();
            assert!(raw.call("flags 32").is_err(), "{phase}/{reply}");
            drop(raw);
        }
        let child = fs::read_to_string(pid).unwrap();
        assert!(
            !Path::new("/proc").join(child).exists(),
            "unreaped child {phase}/{reply}"
        );
    }
}

#[test]
fn every_original_widget_family_stops_at_native_abi_error() {
    struct Stop {
        calls: usize,
        limit: usize,
    }
    impl Canvas for Stop {
        fn call(&mut self, _: &str) -> Result<()> {
            self.calls += 1;
            if self.calls == self.limit {
                Err("owned injected ABI error".into())
            } else {
                Ok(())
            }
        }
        fn measure(&mut self, _: &str, _: f64) -> Result<f64> {
            Err("measurement must not follow failed setup".into())
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let scenes: Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/scenes.json")).unwrap();
    for (name, scene) in scenes.as_object().unwrap() {
        for limit in 1..=3 {
            let mut stop = Stop { calls: 0, limit };
            assert!(
                render_input(
                    &root,
                    &serde_json::to_vec(scene).unwrap(),
                    Path::new("/unused.png"),
                    &mut stop
                )
                .is_err(),
                "{name}/{limit}"
            );
            assert_eq!(stop.calls, limit, "{name}/{limit}");
        }
    }
}

#[test]
fn independent_bitmap_font_policy_matches_actual_pillow_original_rasters() {
    use sha2::{Digest, Sha256};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let font =
        std::fs::read(root.join("openpilot/selfdrive/assets/fonts/NotoColorEmoji.ttf")).unwrap();
    let cases: Value = serde_json::from_str(include_str!(
        "fixtures/ui-forms/emoji-rasters-original.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let pixels = openpilot_dev_tools::ui_forms_reference::emoji_raster(
            &font,
            case["text"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(pixels)),
            case["rgba_sha256"].as_str().unwrap(),
            "{}",
            case["text"]
        );
    }
}

#[test]
fn actual_cli_preserves_raw_python_codepoints_and_lazy_encoding_when_available() {
    use openpilot_dev_tools::ui_forms_check::state_difference;
    use openpilot_log_format::{JsonValue, JsonView};
    use std::process::Command;
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut cases = Vec::new();
    for fixture in [
        include_str!("fixtures/ui-forms/codepoints-original.json"),
        include_str!("fixtures/ui-forms/codepoints-neighbors-original.json"),
    ] {
        let captures = JsonValue::parse(fixture).unwrap();
        let JsonView::Array(captured) = captures.view() else {
            panic!("original codepoint fixture must list")
        };
        cases.extend(captured);
    }
    for case in cases {
        let name = case.get("name").unwrap().to_utf8().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let scene = temp.path().join("scene.json");
        let output = temp.path().join("render.png");
        let bytes = case.get("scene").unwrap().to_json().unwrap();
        std::fs::write(&scene, bytes).unwrap();
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
        let png = case.get("png").unwrap().to_json().unwrap() == "true";
        assert_eq!(output.exists(), png, "{name} PNG");
        assert_eq!(
            output.with_extension("json").exists(),
            case.get("state").unwrap().to_json().unwrap() == "true",
            "{name} state artifact"
        );
        if success {
            use sha2::{Digest, Sha256};
            let expected = case.get("states").unwrap();
            let actual =
                JsonValue::parse(&std::fs::read_to_string(output.with_extension("json")).unwrap())
                    .unwrap();
            assert_eq!(
                state_difference(&actual, &expected).unwrap(),
                None,
                "{name} lossless state"
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(std::fs::read(&output).unwrap())),
                case.get("png_sha256").unwrap().to_utf8().unwrap(),
                "{name} pixels"
            );
        }
    }
}

#[test]
fn input_keeps_distinct_python_surrogates_through_edit_mask_and_snapshot() {
    use openpilot_dev_tools::ui_forms_input::{json_codepoints, Input};
    use openpilot_log_format::{JsonValue, JsonView};
    struct NoMeasure;
    impl Canvas for NoMeasure {
        fn call(&mut self, _: &str) -> Result<()> {
            panic!("storage must not draw")
        }
        fn measure(&mut self, _: &str, _: f64) -> Result<f64> {
            panic!("zero visible width must not encode or measure")
        }
    }
    let mut canvas = NoMeasure;
    let mut input = Input::new(255, true);
    input
        .add_codepoints(&[0xd800], &mut canvas, 0.0, 1.16)
        .unwrap();
    input
        .add_codepoints(&[0xdc00], &mut canvas, 0.0, 1.16)
        .unwrap();
    input.add("x", &mut canvas, 0.0, 1.16).unwrap();
    assert_eq!(input.text, [0xd800, 0xdc00, 120]);
    assert_eq!(input.display(0.0).unwrap(), "••x");
    let state = input.snapshot_json(0.0).unwrap();
    let text = state.get("text").unwrap();
    let JsonView::Text(points) = text.view() else {
        panic!("text expected")
    };
    assert_eq!(points, [0xd800, 0xdc00, 120]);
    let retained = JsonValue::parse_codepoints(&json_codepoints(&state).unwrap()).unwrap();
    let text = retained.get("text").unwrap();
    let JsonView::Text(points) = text.view() else {
        panic!("text expected")
    };
    assert_eq!(points, [0xd800, 0xdc00, 120]);
    input.clear();
    assert!(input.text.is_empty());
}
