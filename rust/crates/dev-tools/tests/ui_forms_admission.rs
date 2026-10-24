//! Independent original-helper admission captures, including successful Python coercion.
#![cfg(feature = "image-array-native")]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

struct PngEvidence<'a> {
    case: &'a Value,
    scene: &'a Path,
    png: &'a Path,
    command: &'a Command,
    observed: &'a Output,
    helper: &'a Path,
    root: &'a Path,
    replay: Option<&'a confirm_draw::Expected>,
}
impl PngEvidence<'_> {
    fn hash(&self, failure_output: Option<&Path>) -> String {
        let actual = format!("{:x}", Sha256::digest(fs::read(self.png).unwrap()));
        let saved = failure_output
            .filter(|_| {
                self.replay
                    .map_or(self.case["png_sha256"].as_str(), |v| Some(v.hash.as_str()))
                    != Some(actual.as_str())
            })
            .map(|output| self.persist(output, &actual));
        if let Some(Err(error)) = saved {
            eprintln!("could not retain Forms PNG mismatch evidence: {error}");
        }
        actual
    }
    fn persist(&self, output: &Path, actual: &str) -> Result<(), Box<dyn std::error::Error>> {
        let name = self.case["name"].as_str().unwrap_or("unnamed");
        let name = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let output = output.join(name);
        fs::create_dir_all(&output)?;
        fs::copy(self.png, output.join("actual.png"))?;
        if let Some(replay) = self.replay {
            fs::copy(&replay.path, output.join("expected-renderer.png"))?;
            fs::write(output.join("expected-replay.stdout.log"), &replay.stdout)?;
            fs::write(output.join("expected-replay.stderr.log"), &replay.stderr)?;
            fs::write(
                output.join("original-draw-capture.json"),
                replay.capture.as_bytes(),
            )?;
        }
        fs::copy(self.scene, output.join("scene.json"))?;
        let state = self.png.with_extension("json");
        if state.is_file() {
            fs::copy(state, output.join("actual-state.json"))?;
        }
        fs::write(output.join("stdout.log"), &self.observed.stdout)?;
        fs::write(output.join("stderr.log"), &self.observed.stderr)?;
        fs::write(
            output.join("expected-case.json"),
            serde_json::to_vec_pretty(self.case)?,
        )?;
        fs::write(
            output.join("input-provenance.json"),
            include_bytes!("fixtures/ui-forms/input-provenance.json"),
        )?;
        let mut environment = std::collections::BTreeMap::new();
        for key in [
            "DISPLAY",
            "LIBGL_ALWAYS_SOFTWARE",
            "GALLIUM_DRIVER",
            "LP_NUM_THREADS",
            "LP_NATIVE_VECTOR_WIDTH",
            "MESA_GL_VERSION_OVERRIDE",
            "MESA_GLSL_VERSION_OVERRIDE",
            "LANG",
            "LC_ALL",
            "LC_NUMERIC",
            "LD_LIBRARY_PATH",
            "STARTUP_UI_RAYLIB_ROOT",
            "STARTUP_UI_RAYLIB_LIBRARY",
            "UI_FORMS_RAW_HELPER",
            "OPENPILOT_RUST_ROOT",
            "RECORD",
            "BURN_IN",
            "PROFILE_STARTUP",
            "PROFILE_RENDER",
            "GRID",
            "SHOW_FPS",
            "SHOW_TOUCHES",
            "STRICT_MODE",
            "ENABLE_VSYNC",
        ] {
            environment.insert(
                key.to_owned(),
                std::env::var_os(key).map(|v| v.to_string_lossy().into_owned()),
            );
        }
        for (key, value) in self.command.get_envs() {
            let key = key.to_string_lossy().into_owned();
            if environment.contains_key(&key) {
                environment.insert(key, value.map(|v| v.to_string_lossy().into_owned()));
            }
        }
        let file_hash = |path: &Path| {
            fs::read(path)
                .map(|v| format!("{:x}", Sha256::digest(v)))
                .ok()
        };
        let library = environment
            .get("STARTUP_UI_RAYLIB_LIBRARY")
            .and_then(Option::as_ref)
            .map(Path::new);
        let pixels = match openpilot_dev_tools::image_array::read(self.png) {
            Ok(image) => {
                let mut hash = Sha256::new();
                for value in image.values {
                    hash.update(value.to_le_bytes());
                }
                serde_json::json!({"mode":image.mode,"shape":image.shape,"i64_sha256":format!("{:x}",hash.finalize())})
            }
            Err(error) => serde_json::json!({"decode_error":error.to_string()}),
        };
        let raylib_hash = library.and_then(file_hash);
        let metadata = serde_json::json!({
            "expected_png_sha256":self.case["png_sha256"],"renderer_expected_png_sha256":self.replay.map(|v| &v.hash),"actual_png_sha256":actual,
            "png_bytes":fs::metadata(self.png)?.len(),"pixels":pixels,
            "environment":environment,"exit_status":self.observed.status.code(),
            "helper_path":self.helper.to_string_lossy(),"helper_sha256":file_hash(self.helper),
            "reference_path":self.command.get_program().to_string_lossy(),
            "reference_sha256":file_hash(Path::new(self.command.get_program())),
            "raylib_sha256":raylib_hash,
            "cargo_lock_sha256":file_hash(&self.root.join("rust/Cargo.lock")),
        });
        fs::write(
            output.join("render-metadata.json"),
            serde_json::to_vec_pretty(&metadata)?,
        )?;
        eprintln!("Forms PNG mismatch evidence: {}", output.display());
        Ok(())
    }
}

#[cfg(unix)]
#[test]
fn png_failure_evidence_is_retained_only_on_mismatch() {
    use std::os::unix::process::ExitStatusExt;
    let temp = tempfile::tempdir().unwrap();
    let scene = temp.path().join("scene.json");
    let png = temp.path().join("actual.png");
    fs::write(&scene, br#"{"kind":"controlled-test"}"#).unwrap();
    let bytes = include_bytes!(
        "fixtures/ui-forms/blobs/704c213a9c24c9a4c47ab84d571022d83c988111f9d07b05bdb66103eb11f09b"
    );
    fs::write(&png, bytes).unwrap();
    fs::write(png.with_extension("json"), b"[]").unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let command = Command::new("controlled-reference");
    let observed = Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout: b"driver stdout".to_vec(),
        stderr: b"GL driver details".to_vec(),
    };
    let actual = format!("{:x}", Sha256::digest(bytes));
    let mut case = serde_json::json!({"name":"controlled-case","png_sha256":actual});
    let output = temp.path().join("evidence");
    let hash = |case: &Value| {
        PngEvidence {
            case,
            scene: &scene,
            png: &png,
            command: &command,
            observed: &observed,
            helper: &scene,
            root: &root,
            replay: None,
        }
        .hash(Some(&output))
    };
    assert_eq!(hash(&case), actual);
    assert!(
        !output.exists(),
        "successful PNG comparison creates no evidence"
    );
    case["png_sha256"] = "controlled-wrong-hash".into();
    assert_eq!(
        hash(&case),
        actual,
        "diagnostics never replace the observed hash"
    );
    let saved = output.join("controlled-case");
    assert_eq!(fs::read(saved.join("actual.png")).unwrap(), bytes);
    assert_eq!(
        fs::read(saved.join("scene.json")).unwrap(),
        fs::read(&scene).unwrap()
    );
    assert_eq!(fs::read(saved.join("stdout.log")).unwrap(), observed.stdout);
    assert_eq!(fs::read(saved.join("stderr.log")).unwrap(), observed.stderr);
    let metadata: Value =
        serde_json::from_slice(&fs::read(saved.join("render-metadata.json")).unwrap()).unwrap();
    assert_eq!(metadata["expected_png_sha256"], "controlled-wrong-hash");
    assert_eq!(metadata["actual_png_sha256"], actual);
    assert!(metadata["pixels"]["shape"].is_array());
    assert!(metadata["helper_sha256"].is_string());
    assert!(saved.join("input-provenance.json").is_file());
}
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
            let failure_output = std::env::var_os("UI_FORMS_FAILURE_OUTPUT");
            // Every successful original raster has an immutable independently
            // captured command stream. Replay uses this process's renderer while
            // the original full state and all font measurements remain exact.
            let replay = Some(
                confirm_draw::replay_admission(
                    case,
                    &root,
                    Path::new(&helper),
                    &command,
                    temp.path(),
                )
                .unwrap(),
            );
            let expected = replay
                .as_ref()
                .map_or_else(|| case["png_sha256"].as_str().unwrap(), |v| v.hash.as_str());
            let evidence = PngEvidence {
                case,
                scene: &scene,
                png: &png,
                command: &command,
                observed: &observed,
                helper: Path::new(&helper),
                root: &root,
                replay: replay.as_ref(),
            };
            assert_eq!(
                evidence.hash(failure_output.as_deref().map(Path::new)),
                expected,
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

#[test]
fn immutable_confirm_draw_capture_binds_scene_source_state_and_rejects_mutations() {
    let text = include_str!("fixtures/ui-forms/confirm-original-draw.json");
    let trace: Value = serde_json::from_str(text).unwrap();
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/admission-original.json")).unwrap();
    let case = cases["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "irrelevant-input-prop")
        .unwrap();
    confirm_draw::validate(text, case).unwrap();
    for field in [
        "scene",
        "state",
        "measurements",
        "commands",
        "original_sha256",
        "provenance_sha256",
    ] {
        let mut changed = trace.clone();
        changed[field] = Value::Null;
        assert!(
            confirm_draw::validate(&serde_json::to_string(&changed).unwrap(), case).is_err(),
            "{field}"
        );
    }
    for field in ["scene", "states", "png_sha256"] {
        let mut changed = case.clone();
        changed[field] = Value::Null;
        assert!(
            confirm_draw::validate(text, &changed).is_err(),
            "case {field}"
        );
    }
}

#[path = "support/ui_forms_confirm_draw.rs"]
mod confirm_draw;

#[test]
fn captured_confirm_replays_exactly_and_a_single_rgb_sample_mutation_is_rejected_when_available() {
    let Some(helper) = std::env::var_os("UI_FORMS_RAW_HELPER") else {
        return;
    };
    let captured: Value =
        serde_json::from_str(include_str!("fixtures/ui-forms/admission-original.json")).unwrap();
    let case = captured["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "irrelevant-input-prop")
        .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    let scene = temp.path().join("scene.json");
    fs::write(&scene, serde_json::to_vec(&case["scene"]).unwrap()).unwrap();
    let png = temp.path().join("actual.png");
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
    let observed = command.output().unwrap();
    assert!(
        observed.status.success(),
        "{}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let state: Value =
        serde_json::from_slice(&fs::read(png.with_extension("json")).unwrap()).unwrap();
    assert_eq!(state, case["states"]);
    let replay =
        confirm_draw::replay(case, &root, Path::new(&helper), &command, temp.path()).unwrap();
    let evidence = || PngEvidence {
        case,
        scene: &scene,
        png: &png,
        command: &command,
        observed: &observed,
        helper: Path::new(&helper),
        root: &root,
        replay: Some(&replay),
    };
    assert_eq!(evidence().hash(None), replay.hash);
    let original = openpilot_dev_tools::image_array::read(&png).unwrap();
    let mut reader = png::Decoder::new(std::io::BufReader::new(fs::File::open(&png).unwrap()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgba, png::BitDepth::Eight)
    );
    pixels.truncate(info.buffer_size());
    pixels[0] ^= 1;
    drop(reader);
    let mut encoder = png::Encoder::new(fs::File::create(&png).unwrap(), info.width, info.height);
    encoder.set_color(info.color_type);
    encoder.set_depth(info.bit_depth);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    let mutated = openpilot_dev_tools::image_array::read(&png).unwrap();
    assert_eq!(
        original
            .values
            .iter()
            .zip(&mutated.values)
            .filter(|(a, b)| a != b)
            .count(),
        1,
        "exactly one RGB sample changed"
    );
    assert_ne!(
        evidence().hash(None),
        replay.hash,
        "exact comparison must reject even one changed sample"
    );
}

#[test]
fn immutable_admission_draws_cover_every_successful_raster_and_bind_all_original_inputs() {
    use std::collections::BTreeSet;
    let text = include_str!("fixtures/ui-forms/admission-original-draw.json");
    let captured: Value = serde_json::from_str(text).unwrap();
    let mut names = BTreeSet::new();
    let mut successful = 0;
    for source in [
        include_str!("fixtures/ui-forms/admission-original.json"),
        include_str!("fixtures/ui-forms/scene-admission-original.json"),
    ] {
        let original: Value = serde_json::from_str(source).unwrap();
        for case in original["cases"].as_array().unwrap() {
            if case["exit"] == 0 && case["png"] == true {
                successful += 1;
                let trace = confirm_draw::validate_admission(text, case).unwrap();
                assert!(names.insert(case["name"].as_str().unwrap().to_owned()));
                for field in ["scene", "scene_text", "states", "env", "png_sha256", "name"] {
                    let mut changed = case.clone();
                    changed[field] = "changed original input".into();
                    assert!(
                        confirm_draw::validate_admission(text, &changed).is_err(),
                        "{}: {field}",
                        case["name"]
                    );
                }
                assert!(trace["commands"].as_array().unwrap().len() >= 10);
            } else {
                assert!(confirm_draw::validate_admission(text, case).is_err());
            }
        }
    }
    assert_eq!(successful, 44);
    assert_eq!(captured["captures"].as_array().unwrap().len(), successful);
    for field in ["captures", "original_sha256", "provenance_sha256"] {
        let mut changed = captured.clone();
        changed[field] = Value::Null;
        assert!(
            confirm_draw::validate_admission(
                &serde_json::to_string(&changed).unwrap(),
                &serde_json::json!({})
            )
            .is_err(),
            "{field}"
        );
    }
    for field in ["commands", "measurements", "scene_sha256", "state"] {
        let mut changed = captured.clone();
        changed["captures"][0][field] = Value::Null;
        assert!(
            confirm_draw::validate_admission(
                &serde_json::to_string(&changed).unwrap(),
                &serde_json::json!({})
            )
            .is_err(),
            "{field}"
        );
    }
}
