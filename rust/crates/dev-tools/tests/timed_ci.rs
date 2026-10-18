//! Preserve the complete timed gate and every unrelated support command.
use openpilot_dev_tools::ci_policy::parse;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const TOOLS: [&str; 8] = [
    "reference",
    "dates",
    "http",
    "logging",
    "daemon",
    "date-daemon",
    "lifecycle",
    "shutdown",
];
const HELPERS: [&str; 3] = [
    "timed-source-reference",
    "timed-command-fixture",
    "timed-runtime-worker",
];
const TESTS: [&str; 17] = [
    "timed_fixtures",
    "timed_shutdown_phase",
    "timed_reference",
    "timed_reference_actions",
    "timed_http_reference",
    "timed_http_report",
    "timed_proxy_reference",
    "timed_native_capture",
    "timed_native_logging",
    "timed_check",
    "timed_check_cli",
    "timed_check_prefix",
    "timed_command_publication",
    "timed_live_child",
    "timed_live",
    "timed_live_cli",
    "timed_live_cleanup",
];
const BUILD_STEP: &str = "Build and test native timed QA tools";
const GUARD: &str =
    "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib ";
fn original() -> Value {
    let bytes = include_bytes!("fixtures/timed-tools/support-ci-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "de88b8b88cb5807b8ff2522adfca1789aa45276742dd621602f48f880d517f2e"
    );
    serde_json::from_slice(bytes).unwrap()
}
fn workflow() -> Value {
    parse(include_str!("../../../../.github/workflows/rust.yml")).unwrap()
}
fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn route(kind: &str) -> String {
    let entry = if ["reference", "dates", "http", "logging", "daemon"].contains(&kind) {
        "examples/timed_fixture"
    } else {
        "openpilot-timed"
    };
    format!("rust/target/debug/check-timed-{kind} --binary rust/target/debug/{entry} --output \"$RUNNER_TEMP/support-runtime/timed-{kind}\"")
}
fn mandatory(step: &Value) -> Result<(), String> {
    require(
        step.get("if").is_none() && step.get("continue-on-error").is_none(),
        "conditional or optional timed gate",
    )
}
fn validate(workflow: &Value) -> Result<(), String> {
    let job = &workflow["jobs"]["support-runtime"];
    mandatory(job)?;
    let baseline = original();
    // Native journal/timed builds and their full regressions exceed the old
    // job-wide budget. Per-case deadlines and every original command stay fixed.
    require(
        baseline["timeout_minutes"] == "30" && job["timeout-minutes"] == "45",
        "support full-regression job budget changed",
    )?;
    for key in [
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
    ] {
        require(job["env"][key] == "0", "unbounded support build artifacts")?;
    }
    for (key, value) in baseline["environment"]
        .as_object()
        .ok_or("missing captured environment")?
    {
        require(
            job["env"][key] == *value,
            "unrelated support environment changed",
        )?;
    }
    let steps = job["steps"].as_array().ok_or("support steps missing")?;
    let scripts = steps
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>();
    let all = scripts.join("\n");
    for name in TOOLS
        .into_iter()
        .map(|name| format!("check_timed_{}.py", name.replace('-', "_")))
        .chain(
            [
                "timed_fixtures.py",
                "timed_reference.py",
                "timed_source_live.py",
                "timed_shutdown_phase.py",
                "test_timed_shutdown_phase.py",
            ]
            .map(str::to_owned),
        )
    {
        require(
            !all.contains(&name),
            &format!("retired Python timed caller remains: {name}"),
        )?;
    }
    let actual_lines = all.lines().map(str::trim).collect::<Vec<_>>();
    let mut required = BTreeMap::<&str, usize>::new();
    for line in baseline["command_lines"]
        .as_array()
        .ok_or("missing captured commands")?
    {
        *required
            .entry(line.as_str().ok_or("bad captured command")?)
            .or_default() += 1;
    }
    for (line, count) in required {
        require(
            actual_lines
                .iter()
                .filter(|candidate| **candidate == line)
                .count()
                >= count,
            &format!("unrelated support command lost: {line}"),
        )?;
    }
    let apt = steps
        .iter()
        .find(|step| {
            step["run"].as_str().is_some_and(|run| {
                run.starts_with("sudo apt-get update && sudo apt-get install -y ")
            })
        })
        .ok_or("native IPC prerequisites missing")?;
    mandatory(apt)?;
    let packages = apt["run"]
        .as_str()
        .ok_or("apt script missing")?
        .split_whitespace()
        .collect::<Vec<_>>();
    for package in baseline["apt_packages"]
        .as_array()
        .ok_or("captured apt prerequisites missing")?
        .iter()
        .filter_map(Value::as_str)
        .chain(["strace"])
    {
        require(
            packages.contains(&package),
            &format!("support prerequisite lost: {package}"),
        )?;
    }
    let build_index = steps
        .iter()
        .position(|step| step["name"] == BUILD_STEP)
        .ok_or("native timed build/test step missing")?;
    let build = &steps[build_index];
    mandatory(build)?;
    require(
        build["env"]["TIMED_NATIVE_ENTRY"]
            == "${{ github.workspace }}/rust/target/debug/openpilot-timed",
        "actual lifecycle entrypoint test prerequisite missing",
    )?;
    let script = build["run"]
        .as_str()
        .ok_or("timed native build commands missing")?;
    let lines = script
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let operations = [("build", 2048), ("test", 512), ("clippy", 512)];
    for (operation, growth) in operations {
        let prefix=format!("cargo +1.94.0 {operation} --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features timed-live-native ");
        let index = lines
            .iter()
            .position(|line| line.starts_with(&prefix))
            .ok_or_else(|| format!("native timed {operation} missing"))?;
        require(
            index > 0 && lines[index - 1] == format!("{GUARD}{growth}"),
            &format!("fresh timed {operation} disk guard missing"),
        )?;
        let line = lines[index];
        require(
            line.ends_with(if operation == "clippy" {
                "--locked -- -D warnings"
            } else {
                "--locked"
            }),
            "softened or unlocked timed command",
        )?;
        for helper in HELPERS {
            require(
                line.contains(&format!("--bin {helper} ")),
                &format!("timed {operation} helper omitted: {helper}"),
            )?;
        }
        for kind in TOOLS {
            require(
                line.contains(&format!("--bin check-timed-{kind} ")),
                &format!("timed {operation} checker omitted: {kind}"),
            )?;
        }
        if operation != "build" {
            for name in TESTS {
                require(
                    line.contains(&format!("--test {name} ")),
                    &format!("timed {operation} regression omitted: {name}"),
                )?;
            }
        }
    }
    require(
        !script.contains("||") && !script.contains("if ") && !script.contains("set +e"),
        "softened timed build/test gate",
    )?;
    for kind in TOOLS {
        let command = route(kind);
        let matching = steps
            .iter()
            .enumerate()
            .filter(|(_, step)| {
                step["run"]
                    .as_str()
                    .is_some_and(|run| run.lines().any(|line| line.trim() == command))
            })
            .collect::<Vec<_>>();
        require(
            matching.len() == 1,
            &format!("native timed {kind} command must run exactly once"),
        )?;
        let (position, step) = matching[0];
        mandatory(step)?;
        require(
            position > build_index,
            "timed checker runs before current helper build",
        )?;
        let run = step["run"].as_str().ok_or("checker run missing")?;
        require(
            !run.contains("||") && !run.contains("if ") && !run.contains("set +e"),
            "softened timed checker",
        )?;
    }
    Ok(())
}
#[test]
fn all_eight_native_timed_checkers_preserve_real_ipc_regressions_and_support_gates() {
    validate(&workflow()).unwrap();
}
#[test]
fn missing_softened_or_reordered_timed_routes_and_regressions_cannot_pass() {
    let baseline = workflow();
    validate(&baseline).expect("actual migration must pass before mutation checks");
    for marker in TOOLS
        .map(route)
        .into_iter()
        .chain(HELPERS.map(|helper| format!("--bin {helper} ")))
        .chain(TESTS.map(|name| format!("--test {name} ")))
        .chain([
            "--features timed-live-native ".into(),
            "-- -D warnings".into(),
            format!("{GUARD}2048"),
            format!("{GUARD}512"),
            "strace".into(),
        ])
    {
        let mut mutation = baseline.clone();
        for step in mutation["jobs"]["support-runtime"]["steps"]
            .as_array_mut()
            .unwrap()
        {
            if let Some(run) = step["run"].as_str() {
                step["run"] = Value::String(run.replace(&marker, "REMOVED"));
            }
        }
        assert!(validate(&mutation).is_err(), "missing {marker} admitted");
    }
    for job_key in ["if", "continue-on-error"] {
        let mut mutation = baseline.clone();
        mutation["jobs"]["support-runtime"][job_key] = Value::String("false".into());
        assert!(validate(&mutation).is_err());
    }
    for minutes in ["0", "30", "46"] {
        let mut mutation = baseline.clone();
        mutation["jobs"]["support-runtime"]["timeout-minutes"] = minutes.into();
        assert!(
            validate(&mutation).is_err(),
            "wrong full-suite budget admitted"
        );
    }
    for key in [
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
    ] {
        let mut mutation = baseline.clone();
        mutation["jobs"]["support-runtime"]["env"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(validate(&mutation).is_err(), "missing build bound {key}");
        mutation["jobs"]["support-runtime"]["env"][key] = "1".into();
        assert!(validate(&mutation).is_err(), "unbounded build {key}");
    }
    for index in 0..baseline["jobs"]["support-runtime"]["steps"]
        .as_array()
        .unwrap()
        .len()
    {
        let step = &baseline["jobs"]["support-runtime"]["steps"][index];
        if step["name"] != BUILD_STEP
            && !step["run"]
                .as_str()
                .is_some_and(|run| run.contains("rust/target/debug/check-timed-"))
        {
            continue;
        }
        for key in ["if", "continue-on-error"] {
            let mut mutation = baseline.clone();
            mutation["jobs"]["support-runtime"]["steps"][index][key] = Value::String("true".into());
            assert!(validate(&mutation).is_err(), "optional step admitted");
        }
        let mut mutation = baseline.clone();
        let run = mutation["jobs"]["support-runtime"]["steps"][index]["run"]
            .as_str()
            .unwrap()
            .to_owned();
        mutation["jobs"]["support-runtime"]["steps"][index]["run"] =
            Value::String(format!("{run} || true"));
        assert!(validate(&mutation).is_err(), "softened timed step admitted");
    }
    let mut mutation = baseline.clone();
    let steps = mutation["jobs"]["support-runtime"]["steps"]
        .as_array_mut()
        .unwrap();
    let index = steps
        .iter()
        .position(|step| step["name"] == BUILD_STEP)
        .unwrap();
    let build = steps.remove(index);
    steps.push(build);
    assert!(
        validate(&mutation).is_err(),
        "late native helper build admitted"
    );
    let mut mutation = baseline.clone();
    let build = mutation["jobs"]["support-runtime"]["steps"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|step| step["name"] == BUILD_STEP)
        .unwrap();
    build["env"]
        .as_object_mut()
        .unwrap()
        .remove("TIMED_NATIVE_ENTRY");
    assert!(
        validate(&mutation).is_err(),
        "missing native lifecycle prerequisite admitted"
    );
}
