//! Telemetry integer retirement must run both the live source and candidate.
use openpilot_dev_tools::ci_policy::parse;
use serde_json::Value;
use std::{fs, path::Path};
const BUILD:&str = "rust/target/debug/build-beep-integer-reference --output \"$RUNNER_TEMP/telemetry-integer-reference\"";
const CHECK:&str = "rust/target/debug/check-beep-integer --binary rust/target/debug/examples/beep_trace --reference \"$RUNNER_TEMP/telemetry-integer-reference/beep-integer-reference\" --output \"$RUNNER_TEMP/telemetry-runtime/beep-integer\"";
fn required_routing(workflow: &Value) -> Result<(), String> {
    let steps = workflow["jobs"]["telemetry-runtime"]["steps"]
        .as_array()
        .ok_or("missing telemetry steps")?;
    let mut commands = vec![];
    for (index, step) in steps.iter().enumerate() {
        if let Some(script) = step["run"].as_str() {
            for (line, text) in script.lines().enumerate() {
                if text.contains("check_beepd_integer.py") {
                    return Err("retired Python integer checker routed".into());
                }
                if text.trim() == BUILD || text.trim() == CHECK {
                    if step.get("if").is_some() || step.get("continue-on-error").is_some() {
                        return Err(
                            "required integer source comparison is conditional or softened".into(),
                        );
                    }
                    commands.push((index, line, text.trim()));
                }
            }
        }
    }
    if commands.len() != 2 || commands[0].2 != BUILD || commands[1].2 != CHECK {
        return Err("missing, duplicated or reordered integer source/check commands".into());
    }
    Ok(())
}
#[test]
fn telemetry_builds_fresh_original_params_reference_then_requires_native_comparison() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow =
        parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap();
    required_routing(&workflow).unwrap();
}
#[test]
fn missing_skipped_softened_and_wrong_reference_routes_are_rejected() {
    let good = serde_json::json!({"jobs":{"telemetry-runtime":{"steps":[{"run":format!("{BUILD}\n{CHECK}\n")}]}}});
    required_routing(&good).unwrap();
    for mutation in [
        "missing-build",
        "missing-check",
        "reorder",
        "conditional",
        "softened",
        "old-python",
        "wrong-reference",
    ] {
        let mut bad = good.clone();
        let step = &mut bad["jobs"]["telemetry-runtime"]["steps"][0];
        match mutation {
            "missing-build" => step["run"] = CHECK.into(),
            "missing-check" => step["run"] = BUILD.into(),
            "reorder" => step["run"] = format!("{CHECK}\n{BUILD}").into(),
            "conditional" => step["if"] = "always()".into(),
            "softened" => step["continue-on-error"] = true.into(),
            "old-python" => {
                step["run"] =
                    format!("{BUILD}\n{CHECK}\npython rust/tools/check_beepd_integer.py").into()
            }
            "wrong-reference" => {
                step["run"] = format!(
                    "{BUILD}\n{}",
                    CHECK.replace(
                        "/telemetry-integer-reference/beep-integer-reference",
                        "/some-candidate"
                    )
                )
                .into()
            }
            _ => unreachable!(),
        }
        assert!(required_routing(&bad).is_err(), "{mutation}");
    }
}
