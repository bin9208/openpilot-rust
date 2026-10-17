use openpilot_dev_tools::{
    seal::Result,
    ui_polygon_reference::{render, PolygonApi},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Default)]
struct Peer {
    frames: usize,
    captures: usize,
    calls: Vec<serde_json::Value>,
    overflow_calls: Vec<usize>,
    diagnostics: Vec<u8>,
    warning_action: Option<openpilot_dev_tools::ui_polygon_reference::WarningAction>,
}
impl Peer {
    fn call(&mut self, name: &str, args: serde_json::Value) {
        self.calls.push(serde_json::json!([name, args]));
    }
}
impl PolygonApi for Peer {
    fn overflow_warning(&mut self) -> Result<()> {
        use openpilot_dev_tools::ui_polygon_reference::{write_overflow_warning, WarningAction};
        self.overflow_calls.push(self.frames);
        write_overflow_warning(
            self.warning_action.unwrap_or(WarningAction::Emit),
            Path::new("SOURCE"),
            &mut self.diagnostics,
        )
    }

    fn initialize(&mut self) -> Result<()> {
        self.call("set_config_flags", serde_json::json!([32]));
        self.call(
            "init_window",
            serde_json::json!([536, 240, "Source ribbon polygons"]),
        );
        self.call("set_target_fps", serde_json::json!([0]));
        Ok(())
    }
    fn begin(&mut self) -> Result<()> {
        self.frames += 1;
        self.call("begin_drawing", serde_json::json!([]));
        Ok(())
    }
    fn clear(&mut self) -> Result<()> {
        self.call("clear_background", serde_json::json!([[0, 0, 0, 255]]));
        Ok(())
    }
    fn load_shader(&mut self, v: &str, f: &str) -> Result<()> {
        self.call("load_shader_from_memory", serde_json::json!([v, f]));
        Ok(())
    }
    fn location(&mut self, name: &str) -> Result<()> {
        self.call("get_shader_location", serde_json::json!([name]));
        Ok(())
    }
    fn integer(&mut self, name: &str, value: i32) -> Result<()> {
        self.call("set_shader_value", serde_json::json!([name, [value], 4, 1]));
        Ok(())
    }
    fn floats(&mut self, name: &str, values: &[f32], components: usize) -> Result<()> {
        let values = values
            .iter()
            .map(|v| {
                if v.is_nan() {
                    serde_json::json!("NaN")
                } else if *v == f32::INFINITY {
                    serde_json::json!("Infinity")
                } else if *v == f32::NEG_INFINITY {
                    serde_json::json!("-Infinity")
                } else {
                    serde_json::json!(f64::from(*v))
                }
            })
            .collect::<Vec<_>>();
        self.call(
            if ["gradientColors", "gradientStops"].contains(&name) {
                "set_shader_value_v"
            } else {
                "set_shader_value"
            },
            serde_json::json!([
                name,
                values,
                match components {
                    1 => 0,
                    2 => 1,
                    4 => 3,
                    _ => panic!(),
                },
                values.len() / components
            ]),
        );
        Ok(())
    }
    fn matrix(&mut self, values: &[f32]) -> Result<()> {
        self.call(
            "set_shader_value_matrix",
            serde_json::json!([values.iter().map(|v| f64::from(*v)).collect::<Vec<_>>()]),
        );
        Ok(())
    }
    fn begin_shader(&mut self) -> Result<()> {
        self.call("begin_shader_mode", serde_json::json!([]));
        Ok(())
    }
    fn strip(&mut self, points: &[f32], color: u32) -> Result<()> {
        self.call(
            "draw_triangle_strip",
            serde_json::json!([
                points
                    .chunks_exact(2)
                    .map(|p| p
                        .iter()
                        .map(|v| if v.is_nan() {
                            serde_json::json!("NaN")
                        } else if *v == f32::INFINITY {
                            serde_json::json!("Infinity")
                        } else if *v == f32::NEG_INFINITY {
                            serde_json::json!("-Infinity")
                        } else {
                            serde_json::json!(f64::from(*v))
                        })
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                points.len() / 2,
                color.to_le_bytes()
            ]),
        );
        Ok(())
    }
    fn end_shader(&mut self) -> Result<()> {
        self.call("end_shader_mode", serde_json::json!([]));
        Ok(())
    }
    fn capture(&mut self, output: &Path) -> Result<()> {
        self.captures += 1;
        self.call("rl_draw_render_batch_active", serde_json::json!([]));
        self.call("load_image_from_screen", serde_json::json!([]));
        self.call(
            "export_image",
            serde_json::json!([output.to_str().unwrap()]),
        );
        self.call("unload_image", serde_json::json!([]));
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        self.call("end_drawing", serde_json::json!([]));
        Ok(())
    }
    fn unload_shader(&mut self) -> Result<()> {
        self.call("unload_shader", serde_json::json!([]));
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.call("close_window", serde_json::json!([]));
        Ok(())
    }
}
#[test]
fn unchanged_original_dynamic_scenes_match_success_and_state_bytes() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-polygon");
    let captured: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures.join("reference-original.json")).unwrap())
            .unwrap();
    let mut issues = Vec::new();
    for row in captured["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let name = row["name"].as_str().unwrap();
        let output = temp.path().join("image.png");
        let mut api = Peer::default();
        let mut printed = vec![];
        let result = render(
            &fixtures.join("input").join(format!("{name}.json")),
            &output,
            &mut api,
            &mut printed,
        );
        if result.is_ok() != (row["exit"] == 0) {
            issues.push(format!("{name}: outcome {result:?}"));
        }
        let trace =
            fs::read_to_string(fixtures.join("original").join(format!("{name}.trace.json")))
                .unwrap();
        let mut expected: Vec<serde_json::Value> = serde_json::from_str(
            &trace
                .replace("-Infinity", "NEGATIVE_INFINITY_TOKEN")
                .replace("Infinity", "\"Infinity\"")
                .replace("NEGATIVE_INFINITY_TOKEN", "\"-Infinity\"")
                .replace("NaN", "\"NaN\""),
        )
        .unwrap();
        // Every captured floating ABI argument is float32. Restore its exact
        // value after serde_json's non-roundtrip decimal reader; do not apply a tolerance.
        fn exact_f32(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Number(n) if n.is_f64() => {
                    *value = serde_json::json!(f64::from(n.as_f64().unwrap() as f32));
                }
                serde_json::Value::Array(v) => {
                    for x in v {
                        exact_f32(x)
                    }
                }
                _ => {}
            }
        }
        for call in &mut expected {
            exact_f32(call);
            if call[0] == "export_image" {
                call[1][0] = serde_json::json!(output.to_str().unwrap());
            }
        }
        for (i, (actual, expected)) in api.calls.iter().zip(&expected).enumerate() {
            let actual_json = actual.to_string();
            let expected_json = expected.to_string();
            if actual_json != expected_json {
                issues.push(format!("{name}: rawcall{i}: {actual} != {expected}"));
                break;
            }
        }
        if api.calls.len() != expected.len() {
            issues.push(format!(
                "{name}: rawcallcount {} != {}",
                api.calls.len(),
                expected.len()
            ));
        }
        let state = output.with_extension("json");
        if !row["state_hex"].is_null() {
            match fs::read(&state) {
                Ok(actual) => {
                    if actual
                        != fs::read(fixtures.join("original").join(format!("{name}.json"))).unwrap()
                    {
                        issues.push(format!("{name}: state bytes"));
                    }
                }
                Err(e) => issues.push(format!("{name}: missing state {e}")),
            }
        } else if state.exists() {
            issues.push(format!("{name}: unexpected state artifact"));
        }
        if api.diagnostics != row["diagnostics"].as_str().unwrap_or("").as_bytes() {
            issues.push(format!(
                "{name}: diagnostic bytes {:?}",
                String::from_utf8_lossy(&api.diagnostics)
            ));
        }
        let expected_warnings = usize::from(!row["diagnostics"].as_str().unwrap_or("").is_empty());
        if api.overflow_calls != vec![0; expected_warnings] {
            issues.push(format!(
                "{name}: warning call phases {:?}",
                api.overflow_calls
            ));
        }
        if printed != row["warnings"].as_str().unwrap().as_bytes() {
            issues.push(format!(
                "{name}: warning bytes {:?}",
                String::from_utf8(printed)
            ));
        }
    }
    assert!(issues.is_empty(), "{}", issues.join("\n"));
}

#[test]
fn original_color_integer_digit_limit_counts_zeros_unicode_and_excludes_signs_and_separators() {
    use sha2::{Digest, Sha256};
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-polygon");
    let original: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/ui-polygon/color-integer-original.json"
    ))
    .unwrap();
    assert_eq!(original["int_max_str_digits"], 4300);
    assert_eq!(
        original["source_sha256"],
        "1be2b79db03c8d5db77ac67714b775527167b3f43e1bf7026888187304a100a8"
    );
    let seed = fs::read(fixtures.join(original["scene"].as_str().unwrap())).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&seed)),
        original["scene_sha256"].as_str().unwrap()
    );
    assert_eq!(original["cases"].as_array().unwrap().len(), 21);
    for row in original["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let scene = temp.path().join("scene.json");
        let output = temp.path().join("output.png");
        let mut input: serde_json::Value = serde_json::from_slice(&seed).unwrap();
        input["colors"] = serde_json::json!([row["color"]]);
        fs::write(&scene, serde_json::to_vec(&input).unwrap()).unwrap();
        let mut api = Peer::default();
        let mut printed = vec![];
        let result = render(&scene, &output, &mut api, &mut printed);
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{}: {result:?}",
            row["name"]
        );
        // The real subprocess capture includes raylib logging; the policy
        // adapter exposes only Python warning output, which these scenes lack.
        assert!(printed.is_empty(), "{}", row["name"]);
        if let Some(hex) = row["state_hex"].as_str() {
            let state = fs::read(output.with_extension("json")).unwrap();
            assert_eq!(
                state.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                hex,
                "{}",
                row["name"]
            );
            assert_eq!(api.frames, 3);
            assert_eq!(api.captures, 1);
        } else {
            assert!(row["stderr"]
                .as_str()
                .unwrap()
                .contains("ValueError: Exceeds the limit (4300 digits)"));
            assert!(!output.with_extension("json").exists());
            assert!(!output.exists());
            assert_eq!(api.frames, 0);
            assert_eq!(api.captures, 0);
            assert_eq!(
                api.calls.len(),
                3,
                "only window initialization precedes int() failure"
            );
        }
    }
}

#[cfg(feature = "polygon-reference")]
#[test]
fn checked_raw_abi_rejects_invalid_scope_lengths_names_and_destroyed_image_tokens() {
    use openpilot_startup_ui::polygon_reference_abi::RawApi;
    use std::os::unix::ffi::OsStringExt;
    let temporary = tempfile::tempdir().unwrap();
    openpilot_disk_space::inspect(temporary.path(), 64 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let library = temporary.path().join("raw.so");
    let raylib = PathBuf::from(
        std::env::var_os("STARTUP_UI_RAYLIB_ROOT").expect("native build raylib root"),
    );
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-polygon/raw-raylib.cc");
    let compiled = std::process::Command::new("g++")
        .args(["-std=c++17", "-shared", "-fPIC"])
        .arg("-I")
        .arg(raylib.join("include"))
        .arg(source)
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    assert!(RawApi::new(Path::new(&std::ffi::OsString::from_vec(
        b"bad\0path".to_vec()
    )))
    .is_err());
    let mut api = RawApi::new(&library).unwrap();
    assert!(api.begin().is_err());
    assert!(api.clear(0).is_err());
    assert!(api.unload_shader().is_err());
    api.flags(32).unwrap();
    api.initialize(536, 240, "owned fake window").unwrap();
    api.fps(0).unwrap();
    assert!(api.initialize(536, 240, "duplicate").is_err());
    assert!(api.clear(0).is_err());
    assert!(api.load_capture().is_err());
    assert!(api.begin_shader().is_err());
    api.begin().unwrap();
    assert!(api.begin().is_err());
    api.clear(0).unwrap();
    api.load_shader("vertex", "fragment").unwrap();
    assert!(api.load_shader("duplicate", "duplicate").is_err());
    assert!(api.integer("unknown", 1).is_err());
    assert!(api.location("bad\0name").is_err());
    api.location("value").unwrap();
    assert!(api.floats("value", &[0.0], 4, true).is_err());
    assert!(api.floats("value", &[0.0], 3, true).is_err());
    assert!(api.floats("value", &[0.0, 1.0], 1, false).is_err());
    assert!(api.matrix("value", &[0.0; 15]).is_err());
    assert!(api.strip(&[0.0; 3], 0).is_err());
    api.begin_shader().unwrap();
    assert!(api.begin_shader().is_err());
    assert!(api.unload_shader().is_err());
    assert!(api.end().is_err());
    api.end_shader().unwrap();
    assert!(api.end_shader().is_err());
    api.flush().unwrap();
    let image = api.load_capture().unwrap();
    assert!(api.export_capture(image + 1, Path::new("unused")).is_err());
    assert!(api.unload_capture(image + 1).is_err());
    assert!(api
        .export_capture(
            image,
            Path::new(&std::ffi::OsString::from_vec(b"bad\0path".to_vec()))
        )
        .is_err());
    assert!(api.export_capture(image, Path::new("owned.png")).unwrap());
    api.unload_capture(image).unwrap();
    assert!(api.unload_capture(image).is_err());
    assert!(api.export_capture(image, Path::new("unused")).is_err());
    assert!(api.close().is_err());
    api.end().unwrap();
    assert!(api.close().is_err());
    api.unload_shader().unwrap();
    assert!(api.integer("value", 1).is_err());
    api.close().unwrap();
    assert!(api.close().is_err());
}

#[test]
fn retained_original_helper_shader_and_locked_dependencies_are_live_drift_guards() {
    use openpilot_dev_tools::ui_polygon_reference::validate_source;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let pins: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ui-polygon/policy-pins.json")).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    for name in pins["sources"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("uv.lock"))
    {
        let target = temporary.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(root.join(name), target) {
            Err(error)
                if name == "rust/tools/ui_qa/polygon_source.py"
                    && error.kind() == std::io::ErrorKind::NotFound => {}
            result => {
                result.unwrap();
            }
        }
    }
    validate_source(temporary.path()).unwrap();
    for name in [
        "rust/tools/ui_qa/polygon_source.py",
        "openpilot/system/ui/lib/shader_polygon.py",
    ] {
        let path = temporary.path().join(name);
        let original = fs::read(&path).ok();
        let mut changed = original.clone().unwrap_or_default();
        changed.extend(b"\n# source policy drift\n");
        fs::write(&path, changed).unwrap();
        assert!(validate_source(temporary.path()).is_err(), "{name}");
        if let Some(original) = original {
            fs::write(path, original).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
    let retired = temporary.path().join("rust/tools/ui_qa/polygon_source.py");
    if retired.exists() {
        fs::remove_file(retired).unwrap();
    }
    validate_source(temporary.path()).unwrap();
    let lock = temporary.path().join("uv.lock");
    let original = fs::read_to_string(&lock).unwrap();
    fs::write(
        &lock,
        original.replace("name = \"numpy\"", "name = \"changed-numpy\""),
    )
    .unwrap();
    assert!(validate_source(temporary.path()).is_err());
    fs::write(lock, original).unwrap();
    fs::write(
        temporary.path().join("openpilot/system/ui/__init__.py"),
        b"",
    )
    .unwrap();
    assert!(validate_source(temporary.path()).is_err());
}

#[test]
fn original_numpy_warning_environment_actions_preserve_admission_and_successful_diagnostics() {
    use openpilot_dev_tools::ui_polygon_reference::{warning_action, WarningAction};
    use std::ffi::OsStr;
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-polygon");
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ui-polygon/warning-original.json")).unwrap();
    for row in original.as_array().unwrap() {
        let action = warning_action(row["env"].as_str().map(OsStr::new));
        let mut peer = Peer {
            warning_action: Some(action),
            ..Peer::default()
        };
        let output = tempfile::tempdir().unwrap();
        let image = output.path().join("image.png");
        let mut printed = vec![];
        let result = render(
            &fixtures.join("input/point-float-overflow.json"),
            &image,
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{}: {result:?}",
            row["name"]
        );
        assert_eq!(peer.overflow_calls, vec![0]);
        assert_eq!(image.with_extension("json").exists(), row["state"] == true);
        if row["exit"] == 0 {
            if action == WarningAction::Ignore {
                assert!(peer.diagnostics.is_empty());
            } else {
                let hex = row["stderr_hex"].as_str().unwrap();
                let bytes = hex
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                    .collect::<Vec<_>>();
                let stderr = String::from_utf8(bytes).unwrap();
                let marker = ":19: RuntimeWarning:";
                let i = stderr.find(marker).unwrap();
                assert_eq!(
                    peer.diagnostics,
                    format!("SOURCE{}", &stderr[i..]).as_bytes()
                );
            }
        } else {
            assert!(peer.diagnostics.is_empty());
            assert_eq!(peer.frames, 0);
        }
    }
}

#[test]
fn original_json_recursion_and_numpy_dimension_boundaries_preserve_pre_window_failures() {
    let captured: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ui-polygon/boundary-original.json")).unwrap();
    for row in captured["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let scene = temp.path().join("scene.json");
        let output = temp.path().join("output.png");
        fs::write(&scene, row["input"].as_str().unwrap()).unwrap();
        let mut api = Peer::default();
        let mut printed = vec![];
        let result = render(&scene, &output, &mut api, &mut printed);
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{}: {result:?}",
            row["name"]
        );
        assert_eq!(
            output.with_extension("json").exists(),
            row["state"] == true,
            "{}",
            row["name"]
        );
        if row["name"].as_str().unwrap().starts_with("json-") && row["exit"] != 0 {
            assert!(
                api.calls.is_empty(),
                "JSON recursion must fail before window initialization"
            );
        }
    }
}
