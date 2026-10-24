//! Required full framework routing and negative policy mutations.
use openpilot_dev_tools::ci_policy::{parse, TRUSTED_SOURCE};
use serde_json::Value;
use std::{fs, path::Path};
const SUITE:&str="--features ui-application-lifecycle-native --test ui_application_lifecycle_loop --test ui_application_lifecycle_recording --test ui_application_lifecycle_reference --test ui_application_lifecycle_raw --test ui_application_lifecycle_check --test ui_application_lifecycle_native --locked";
const RAW:&str="rust/target/debug/build-ui-application-reference --output \"$RUNNER_TEMP/ui-application-raw\" --raylib-root \"$STARTUP_UI_RAYLIB_ROOT\"";
fn validate(workflow: &Value, caller: &str) -> Result<(), String> {
    if !caller.contains(r#"("application", None, "application_run", true)"#)
        || caller.contains("check_ui_application.py")
    {
        return Err("framework application still selects Python".into());
    }
    let job = &workflow["jobs"]["ui-connectivity"];
    if job.get("if").is_some() || job.get("continue-on-error").is_some() {
        return Err("UI lifecycle job must remain mandatory".into());
    }
    let steps = workflow["jobs"]["ui-connectivity"]["steps"]
        .as_array()
        .ok_or("UI job missing")?;
    let build = steps
        .iter()
        .position(|s| s["name"] == "Build native UI validation checkers")
        .ok_or("native build step missing")?;
    let script = steps[build]["run"]
        .as_str()
        .ok_or("native build commands missing")?;
    for marker in [
        "ui-application-lifecycle-native",
        "--bin check-ui-application",
        "--bin ui-application-reference",
        "--bin build-ui-application-reference",
    ] {
        if !script.contains(marker) {
            return Err(format!("missing {marker}"));
        }
    }
    let raw = steps
        .iter()
        .position(|s| s["run"].as_str().is_some_and(|s| s.contains(RAW)))
        .ok_or("real application/XTest boundary build missing")?;
    let raw_script = steps[raw]["run"].as_str().ok_or("raw command missing")?;
    for marker in [
        "--growth-mib 16",
        "UI_APPLICATION_RAW_HELPER=",
        "\"$GITHUB_ENV\"",
    ] {
        if !raw_script.contains(marker) {
            return Err(format!("missing {marker}"));
        }
    }
    if !script.contains("--growth-mib 2048") {
        return Err("native checker build disk guard missing".into());
    }
    let run = steps
        .iter()
        .position(|s| {
            s["name"] == "Compare shared UI state, pixels, real input and native lifecycle"
        })
        .ok_or("full UI comparison missing")?;
    if raw >= run {
        return Err("application raw build is after its consumer".into());
    }
    let script = steps[run]["run"]
        .as_str()
        .ok_or("comparison command missing")?;
    let xvfb = script.find("xvfb-run").ok_or("owned display missing")?;
    let suite = script
        .find(SUITE)
        .ok_or("lifecycle regression suite missing")?;
    let framework = script
        .find("rust/target/debug/check-ui-framework --python python --target")
        .ok_or("full framework caller missing")?;
    if !script[xvfb..suite].contains("--growth-mib 512") {
        return Err("lifecycle suite disk guard missing".into());
    }
    if xvfb >= suite
        || suite >= framework
        || !script[xvfb..suite].contains("test -x \"$UI_APPLICATION_RAW_HELPER\"")
    {
        return Err(
            "lifecycle suite requires built helper and owned display before framework".into(),
        );
    }
    for index in [build, raw, run] {
        if steps[index]["run"].as_str().is_some_and(|script| {
            ["|| true", "|| :", "set +e"]
                .iter()
                .any(|marker| script.contains(marker))
        }) {
            return Err("lifecycle command suppresses failure".into());
        }
        if steps[index]["if"] != TRUSTED_SOURCE || steps[index].get("continue-on-error").is_some() {
            return Err("lifecycle step must be mandatory".into());
        }
    }
    if !steps.iter().any(|s| {
        s["run"].as_str().is_some_and(|s| {
            s.contains("apt-get -o ") && s.contains(" install ") && s.contains("libxtst6")
        })
    }) {
        return Err("native XTest dependency missing".into());
    }
    Ok(())
}
fn inputs() -> (Value, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    (
        parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap(),
        fs::read_to_string(root.join("rust/crates/dev-tools/src/ui_framework_check.rs")).unwrap(),
    )
}
#[test]
fn mandatory_lifecycle_closure_builds_tests_and_routes_native() {
    let (w, c) = inputs();
    validate(&w, &c).unwrap();
}
#[test]
fn missing_skipped_softened_or_unrouted_lifecycle_checks_fail_policy() {
    let (w, c) = inputs();
    validate(&w, &c).expect("baseline must pass before mutation checks");
    for marker in [
        "ui-application-lifecycle-native",
        "--bin check-ui-application",
        "--bin ui-application-reference",
        "--bin build-ui-application-reference",
        RAW,
        "UI_APPLICATION_RAW_HELPER=",
        "--test ui_application_lifecycle_loop",
        "--test ui_application_lifecycle_recording",
        "--test ui_application_lifecycle_reference",
        "--test ui_application_lifecycle_raw",
        "--test ui_application_lifecycle_check",
        "--test ui_application_lifecycle_native",
        "test -x \"$UI_APPLICATION_RAW_HELPER\"",
        "libxtst6",
    ] {
        let mut mutation = w.clone();
        for step in mutation["jobs"]["ui-connectivity"]["steps"]
            .as_array_mut()
            .unwrap()
        {
            if let Some(script) = step["run"].as_str() {
                step["run"] = Value::String(script.replace(marker, "REMOVED"));
            }
        }
        assert!(
            validate(&mutation, &c).is_err(),
            "missing {marker} admitted"
        );
    }
    for name in [
        "Build native UI validation checkers",
        "Compare shared UI state, pixels, real input and native lifecycle",
    ] {
        for (key, value) in [
            ("if", Value::String("false".into())),
            ("continue-on-error", Value::Bool(true)),
        ] {
            let mut mutation = w.clone();
            let step = mutation["jobs"]["ui-connectivity"]["steps"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|s| s["name"] == name)
                .unwrap();
            step[key] = value;
            assert!(validate(&mutation, &c).is_err(), "{name} {key} admitted");
        }
    }
    for key in ["if", "continue-on-error"] {
        let mut mutation = w.clone();
        mutation["jobs"]["ui-connectivity"][key] = Value::Bool(true);
        assert!(validate(&mutation, &c).is_err(), "job {key} admitted");
    }
    for suffix in [" || true", " || :"] {
        let mut mutation = w.clone();
        for step in mutation["jobs"]["ui-connectivity"]["steps"]
            .as_array_mut()
            .unwrap()
        {
            if let Some(script) = step["run"].as_str() {
                step["run"] = Value::String(script.replace(SUITE, &format!("{SUITE}{suffix}")));
            }
        }
        assert!(validate(&mutation, &c).is_err(), "softened suite admitted");
    }
    let mut mutation = w.clone();
    let steps = mutation["jobs"]["ui-connectivity"]["steps"]
        .as_array_mut()
        .unwrap();
    let raw = steps
        .iter()
        .position(|s| s["run"].as_str().is_some_and(|s| s.contains(RAW)))
        .unwrap();
    let step = steps.remove(raw);
    steps.push(step);
    assert!(validate(&mutation, &c).is_err(), "late raw build admitted");
    assert!(validate(
        &w,
        &c.replace(
            r#"("application", None, "application_run", true)"#,
            r#"("application", Some("check_ui_application.py"), "application_run", true)"#
        )
    )
    .is_err());
}
