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
}
impl PngEvidence<'_> {
    fn hash(&self, failure_output: Option<&Path>) -> String {
        let actual = format!("{:x}", Sha256::digest(fs::read(self.png).unwrap()));
        let saved = failure_output
            .filter(|_| self.case["png_sha256"].as_str() != Some(actual.as_str()))
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
            "expected_png_sha256":self.case["png_sha256"],"actual_png_sha256":actual,
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
            let evidence = PngEvidence {
                case,
                scene: &scene,
                png: &png,
                command: &command,
                observed: &observed,
                helper: Path::new(&helper),
                root: &root,
            };
            assert_eq!(
                evidence.hash(failure_output.as_deref().map(Path::new)),
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
