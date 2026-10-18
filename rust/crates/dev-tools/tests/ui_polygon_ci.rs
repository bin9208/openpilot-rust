//! Production polygon checking must include the full decoder set and its SDK.
use openpilot_dev_tools::ci_policy::parse;
use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn forms_native_reference_runs_real_pixels_with_owned_display_and_raw_helper() {
    let root = root();
    let workflow =
        parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let steps = workflow["jobs"]["ui-connectivity"]["steps"]
        .as_array()
        .unwrap();
    let build = steps
        .iter()
        .find(|step| step["name"] == "Build native UI validation checkers")
        .unwrap();
    let script = build["run"].as_str().unwrap();
    assert!(script.contains("--bin check-ui-forms --bin ui-forms-reference"));
    let raw = steps
        .iter()
        .position(|step| step["name"] == "Build independent Forms drawing boundary")
        .expect("missing Forms raw build");
    let test = steps
        .iter()
        .position(|step| {
            step["name"] == "Compare shared UI state, pixels, real input and native lifecycle"
        })
        .unwrap();
    assert!(raw < test);
    for step in [build, &steps[raw], &steps[test]] {
        assert!(step.get("if").is_none());
        assert!(step.get("continue-on-error").is_none());
    }
    let raw_script = steps[raw]["run"].as_str().unwrap();
    for required in [
        "--growth-mib 16",
        "-Wall -Wextra -Werror",
        "rust/crates/dev-tools/tests/fixtures/ui-forms/raw-raylib.cc",
        "UI_FORMS_RAW_HELPER=",
        "\"$GITHUB_ENV\"",
    ] {
        assert!(raw_script.contains(required), "missing {required}");
    }
    let script = steps[test]["run"].as_str().unwrap();
    let xvfb = script.find("xvfb-run").unwrap();
    let suite = script.find("cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features image-array-native --test ui_forms_check --test ui_forms_reference --test ui_forms_admission --test ui_forms_family_text --test ui_forms_snapshot_order --test ui_render_check --test ui_render_reference --test ui_network_check --test ui_network_reference --test ui_framework_check --locked").expect("missing real Forms tests");
    assert!(xvfb < suite);
    assert!(script[xvfb..suite].contains("test -x \"$UI_FORMS_RAW_HELPER\""));
    assert!(script[xvfb..suite].contains("test -f \"$STARTUP_UI_RAYLIB_LIBRARY\""));
    assert!(
        suite
            < script
                .find("rust/target/debug/check-ui-framework --python python --target")
                .unwrap()
    );
    let caller =
        fs::read_to_string(root.join("rust/crates/dev-tools/src/ui_framework_check.rs")).unwrap();
    assert!(caller.contains(r#"("forms", None, "form_render", true)"#));
    assert!(!caller.contains("check_ui_forms.py"));
}

#[test]
fn native_diagnostics_are_built_tested_and_selected_by_the_complete_ui_gate() {
    let root = root();
    let workflow =
        parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let steps = workflow["jobs"]["ui-connectivity"]["steps"]
        .as_array()
        .unwrap();
    let command = steps
        .iter()
        .filter_map(|s| s["run"].as_str())
        .find(|r| r.contains("--bin check-ui-diagnostics"))
        .expect("diagnostic tools must be built");
    for bin in [
        "--bin check-ui-diagnostics",
        "--bin ui-diagnostic-reference",
        "--bin build-ui-diagnostic-reference",
    ] {
        assert!(command.contains(bin));
    }
    assert!(command.contains("--features egl-reference,polygon-reference"));
    assert!(command.contains("--features image-array-native --test ui_diagnostic_reference --test ui_diagnostic_check --test ui_emoji_check --test ui_emoji_ci --locked"));
    let build = "rust/target/debug/build-ui-diagnostic-reference --output \"$RUNNER_TEMP/ui-diagnostic-raw\" --raylib-root \"$STARTUP_UI_RAYLIB_ROOT\"";
    assert!(command.contains(build));
    assert!(command.contains("UI_DIAGNOSTIC_RAW_HELPER=%s/ui-diagnostic-raw/ui-diagnostic-raw"));
    assert!(command.contains("\"$GITHUB_ENV\""));
    let caller =
        fs::read_to_string(root.join("rust/crates/dev-tools/src/ui_framework_check.rs")).unwrap();
    assert!(caller.contains(r#"("diagnostics", None, "diagnostic_render", true)"#));
    assert!(!caller.contains("check_ui_diagnostics.py"));
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("rust/crates/dev-tools/Cargo.toml")).unwrap())
            .unwrap();
    let bin = manifest["bin"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"].as_str() == Some("check-ui-diagnostics"))
        .unwrap();
    assert_eq!(
        bin["required-features"].as_array().unwrap()[0].as_str(),
        Some("image-array-native")
    );
}

#[test]
fn production_polygon_requires_native_codecs_and_ci_runs_the_real_gate() {
    let root = root();
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("rust/crates/dev-tools/Cargo.toml")).unwrap())
            .unwrap();
    assert!(manifest["features"]["polygon-reference"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some("image-array-native")));
    let workflow =
        parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let steps = workflow["jobs"]["ui-connectivity"]["steps"]
        .as_array()
        .unwrap();
    let stage = steps
        .iter()
        .position(|s| s["uses"] == "./.github/actions/native-image-sdk")
        .unwrap();
    let build = steps
        .iter()
        .position(|s| {
            s["run"]
                .as_str()
                .is_some_and(|r| r.contains("--bin check-ui-polygon --bin ui-polygon-reference"))
        })
        .unwrap();
    assert!(stage < build);
    for step in [&steps[stage], &steps[build]] {
        assert!(step.get("if").is_none());
        assert!(step.get("continue-on-error").is_none());
    }
    let command = steps[build]["run"].as_str().unwrap();
    assert!(command.contains("--features egl-reference,polygon-reference"));
    assert!(command.contains("--features polygon-reference --test image_array --test ui_polygon_reference --test ui_polygon_check --locked"));
    assert!(command.contains("-p openpilot-image-codecs --features native-sdk --locked"));
    let caller =
        fs::read_to_string(root.join("rust/crates/dev-tools/src/ui_framework_check.rs")).unwrap();
    assert!(caller.contains(r#"("polygon", None, "polygon_render", true)"#));
    assert!(!caller.contains("check_ui_polygon.py"));
}

#[test]
fn sdk_sources_are_pinned_and_the_action_checks_before_building() {
    let root = root();
    let sdk = root.join("rust/crates/image-codecs/native/sdk");
    let sources: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(sdk.join("sources.json")).unwrap()).unwrap();
    let records = sources["sources"].as_array().unwrap();
    assert_eq!(records.len(), 9);
    let stager = fs::read_to_string(sdk.join("stage.rs")).unwrap();
    let mut names = std::collections::HashSet::new();
    for source in records {
        let archive = source["archive"].as_str().unwrap();
        let hash = source["sha256"].as_str().unwrap();
        assert_eq!(std::path::Path::new(archive).components().count(), 1);
        assert!(names.insert(archive));
        assert!(source["url"].as_str().unwrap().starts_with("https://"));
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(stager.contains(&format!("\"{archive}\"")));
        assert!(stager.contains(&format!("\"{hash}\"")));
    }
    let action = parse(
        &fs::read_to_string(root.join(".github/actions/native-image-sdk/action.yml")).unwrap(),
    )
    .unwrap();
    let command = action["runs"]["steps"][0]["run"].as_str().unwrap();
    let guard = command.find("--growth-mib 2048").unwrap();
    let download = command.find("curl --fail").unwrap();
    let verify = command.find("sha256sum --check --strict").unwrap();
    let compile = command.find("rustc +1.94.0").unwrap();
    assert!(guard < download && download < verify && verify < compile);
    assert!(command.contains("set -euo pipefail"));
    assert!(command.contains("CHECK_DISK_SPACE_BINARY="));
    assert!(command.contains("\"$sdk_root/stager-tests\"\n"));
    assert!(!command.contains("python"));
    assert!(!command.contains("meson"));
}
