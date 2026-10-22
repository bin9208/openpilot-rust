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
const ORIGINAL_LOG_STARTUP: &str = "python rust/tools/check_logmessaged_startup.py --collector rust/target/debug/openpilot-logmessaged --output \"$RUNNER_TEMP/support-runtime/logmessaged-startup\"";
const ORIGINAL_UPLOADER: &str = "python rust/tools/check_uploader.py --binary rust/target/debug/examples/uploader_trace --output \"$RUNNER_TEMP/support-runtime/uploader-reference/report.json\"";
const ORIGINAL_UPLOADER_LOGGING: [&str; 2] = [
    "python rust/tools/check_uploader_logging.py --binary rust/target/debug/openpilot-uploader \\",
    "--trace rust/target/debug/examples/uploader_trace --collector rust/target/debug/openpilot-logmessaged --output \"$RUNNER_TEMP/support-runtime/uploader-logging\"",
];
const NATIVE_UPLOADER: [&str; 4] = [
    "rust/target/debug/check-uploader-reference --root \"$GITHUB_WORKSPACE\" \\",
    "--binary rust/target/debug/examples/uploader_trace --output \"$RUNNER_TEMP/support-runtime/uploader-reference\" \\",
    "--fixture rust/crates/dev-tools/tests/fixtures/uploader-original-group/reference-observations.json \\",
    "--fixture-sha256 b1ce58a70769ab48c7893f26da0f7186b2e7d378fd8a19fd5e495f88d68d4eb5",
];
const NATIVE_UPLOADER_LOGGING: [&str; 5] = [
    "rust/target/debug/check-uploader-logging --root \"$GITHUB_WORKSPACE\" \\",
    "--binary rust/target/debug/openpilot-uploader --trace rust/target/debug/examples/uploader_trace \\",
    "--collector rust/target/debug/openpilot-logmessaged --output \"$RUNNER_TEMP/support-runtime/uploader-logging\" \\",
    "--fixture rust/crates/dev-tools/tests/fixtures/uploader-original-group/logging-observations.json \\",
    "--fixture-sha256 cdf42bbfebd0613cfc4896c419d96927cf410301155d3ce6b2141cb10313abba",
];
const SIBLING_ORIGINAL: &[(&str, &[&str])] = &[
    ("timeouts", &["python rust/tools/check_uploader_timeouts.py --binary rust/target/debug/examples/uploader_trace --output \"$RUNNER_TEMP/support-runtime/uploader-timeouts\""]),
    ("daemon", &["python rust/tools/check_uploader_daemon.py --binary rust/target/debug/openpilot-uploader --output \"$RUNNER_TEMP/support-runtime/uploader-native\""]),
    ("interrupted-http", &["python rust/tools/check_uploader_http_interrupted.py --binary rust/target/debug/openpilot-uploader --output \"$RUNNER_TEMP/support-runtime/uploader-http-interrupted\""]),
    ("params", &["python rust/tools/check_params_string_uploader.py --binding \"$RUNNER_TEMP\"/support-params-binding/params_pyx*.so \\", "--binary rust/target/debug/openpilot-uploader --collector rust/target/debug/openpilot-logmessaged --output \"$RUNNER_TEMP/support-runtime/params-uploader\""]),
    ("fault", &["python rust/tools/check_uploader_log_fault.py --native rust/target/debug/examples/uploader_log_fault \\", "--binding \"$RUNNER_TEMP\"/support-params-binding/params_pyx*.so --collector rust/target/debug/openpilot-logmessaged --output \"$RUNNER_TEMP/support-runtime/uploader-log-fault\""]),
    ("interrupted-send", &["python rust/tools/check_uploader_interrupted.py --binary \"$fault_root/bin/openpilot-uploader\" --output \"$fault_root/uploader\""]),
];
const UPLOADER_BUILD: &str = "cargo build --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features uploader-siblings-native --bin check-uploader-reference --bin check-uploader-logging --bin check-uploader-siblings --locked";
const UPLOADER_HTTP_PROVIDER: [&str; 3] = [
    "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 16",
    "interposer=\"$RUNNER_TEMP/support-runtime/uploader-http-interrupted-provider.so\"",
    "cc -std=c11 -shared -fPIC -Wall -Wextra -Werror rust/tools/fixtures/http/interrupted_read.c -o \"$interposer\" -ldl -pthread",
];
const NATIVE_LOG_STARTUP: &str = "env OPENPILOT_RUST_ROOT=\"$root\" PATH=\"$proof/native-path\" /usr/bin/timeout --kill-after=5s 180s /usr/bin/strace -f -qq -s 65535 -e trace=execve,execveat -o \"$proof/$lane.exec\" \"$checker\" --collector \"$collector\" --output \"$proof/$lane\" > \"$proof/$lane.stdout\" 2> \"$proof/$lane.stderr\"";
const LOG_LANES: &str = "for lane in current absent; do";
const LOG_ABSENT_ROOT: &str = "if [ \"$lane\" = absent ]; then root=\"$roots/absent\"; fi";
const LOG_CHECKER: &str =
    "checker=\"$GITHUB_WORKSPACE/rust/target/debug/check-logmessaged-startup\"";
const LOG_COLLECTOR: &str =
    "collector=\"$GITHUB_WORKSPACE/rust/target/debug/openpilot-logmessaged\"";
const LOG_PROOF: &str = "proof=\"$RUNNER_TEMP/support-runtime/logmessaged-startup-retirement\"";
const LOG_ORIGINAL_ABSENT: &str = "test ! -e rust/tools/check_logmessaged_startup.py";
const LOG_ORIGINAL_NOT_LINK: &str = "test ! -L rust/tools/check_logmessaged_startup.py";
const LOG_COPIED_ABSENT: &str =
    "test ! -e \"$roots/absent/rust/tools/check_logmessaged_startup.py\"";
const LOG_COPIED_NOT_LINK: &str =
    "test ! -L \"$roots/absent/rust/tools/check_logmessaged_startup.py\"";
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

fn sibling_route(scope: &str) -> Vec<String> {
    let provider = match scope {
        "timeouts" => "--trace rust/target/debug/examples/uploader_trace --output \"$RUNNER_TEMP/support-runtime/uploader-timeouts\" \\",
        "daemon" => "--binary rust/target/debug/openpilot-uploader --output \"$RUNNER_TEMP/support-runtime/uploader-native\" \\",
        "interrupted-http" => "--binary rust/target/debug/openpilot-uploader --http-provider \"$interposer\" --output \"$RUNNER_TEMP/support-runtime/uploader-http-interrupted\" \\",
        "params" => "--binary rust/target/debug/openpilot-uploader --collector rust/target/debug/openpilot-logmessaged --output \"$RUNNER_TEMP/support-runtime/params-uploader\" \\",
        "fault" => "--fault rust/target/debug/examples/uploader_log_fault --collector rust/target/debug/openpilot-logmessaged --output \"$RUNNER_TEMP/support-runtime/uploader-log-fault\" \\",
        "interrupted-send" => "--interrupted \"$fault_root/bin/openpilot-uploader\" --output \"$fault_root/uploader\" \\",
        _ => panic!("closed uploader sibling scope"),
    };
    vec![
        format!("rust/target/debug/check-uploader-siblings --root \"$GITHUB_WORKSPACE\" --scope {scope} \\"),
        provider.into(),
        "--fixture rust/crates/dev-tools/tests/fixtures/uploader-sibling-original-group/source-observations.json \\".into(),
        "--fixture-sha256 30f9e1f21c47cd9bcefcc161456e1df9984338cc2a2e940eabb8dce0a97a6176".into(),
    ]
}
fn uploader_route_step<'a>(
    steps: &'a [Value],
    native: &[String],
) -> Result<(usize, &'a Value), String> {
    let matching = steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            let run = step["run"].as_str()?;
            let lines = run.lines().map(str::trim).collect::<Vec<_>>();
            let count = lines
                .windows(native.len())
                .filter(|lines| lines.iter().copied().eq(native.iter().map(String::as_str)))
                .count();
            (count > 0).then_some((index, step, count))
        })
        .collect::<Vec<_>>();
    require(
        matching.len() == 1 && matching[0].2 == 1,
        "native uploader route missing or duplicated",
    )?;
    let (index, step, _) = matching[0];
    mandatory(step)?;
    let run = step["run"]
        .as_str()
        .ok_or("native uploader script missing")?;
    require(
        run.starts_with("set -euo pipefail\n")
            && !run.contains("||")
            && !run.contains("set +e")
            && !run.contains("if "),
        "softened native uploader comparison",
    )?;
    if native[0] == sibling_route("interrupted-http")[0] {
        let required = UPLOADER_HTTP_PROVIDER
            .iter()
            .copied()
            .chain(native.iter().map(String::as_str))
            .collect::<Vec<_>>();
        let lines = run.lines().map(str::trim).collect::<Vec<_>>();
        require(
            lines
                .windows(required.len())
                .filter(|lines| *lines == required.as_slice())
                .count()
                == 1,
            "HTTP interposer source/flags/path/guard changed",
        )?;
    }
    let builds = steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            let count = step["run"]
                .as_str()?
                .lines()
                .filter(|line| line.trim() == UPLOADER_BUILD)
                .count();
            (count > 0).then_some((index, step, count))
        })
        .collect::<Vec<_>>();
    require(
        builds.len() == 1 && builds[0].2 == 1,
        "native uploader shared build missing or duplicated",
    )?;
    let (build_index, build, _) = builds[0];
    mandatory(build)?;
    require(
        index >= build_index
            && (index != build_index || run.find(UPLOADER_BUILD) < run.find(native[0].as_str())),
        "native uploader comparison precedes current shared build",
    )?;
    Ok((index, step))
}

fn validate(workflow: &Value) -> Result<(), String> {
    let job = &workflow["jobs"]["support-runtime"];
    mandatory(job)?;
    let baseline = original();
    // Native journal/timed builds and their full regressions exceed the old
    // job-wide budget. Per-case deadlines and unrelated commands stay fixed.
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
        // The original startup command was retired after a sealed original
        // capture. Keep this baseline immutable and admit only the mandatory
        // same-ELF current/absent native checker against the production collector.
        let line = if line == ORIGINAL_LOG_STARTUP {
            require(
                [
                    LOG_LANES,
                    LOG_ABSENT_ROOT,
                    LOG_CHECKER,
                    LOG_COLLECTOR,
                    LOG_PROOF,
                    LOG_ORIGINAL_ABSENT,
                    LOG_ORIGINAL_NOT_LINK,
                    LOG_COPIED_ABSENT,
                    LOG_COPIED_NOT_LINK,
                ]
                .iter()
                .all(|line| actual_lines.contains(line))
                    && !all.contains("python rust/tools/check_logmessaged_startup.py"),
                "native logmessaged retirement lanes, collector or source absence changed",
            )?;
            NATIVE_LOG_STARTUP.to_owned()
        } else if line == ORIGINAL_UPLOADER
            || ORIGINAL_UPLOADER_LOGGING.contains(&line)
            || SIBLING_ORIGINAL.iter().any(|(_, old)| old.contains(&line))
        {
            // Keep the captured baseline immutable. Each retired command maps
            // only to its accepted complete native fixture/provider/argument route.
            let native = if line == ORIGINAL_UPLOADER {
                NATIVE_UPLOADER.map(str::to_owned).to_vec()
            } else if ORIGINAL_UPLOADER_LOGGING.contains(&line) {
                NATIVE_UPLOADER_LOGGING.map(str::to_owned).to_vec()
            } else {
                sibling_route(
                    SIBLING_ORIGINAL
                        .iter()
                        .find(|(_, old)| old.contains(&line))
                        .ok_or("missing original uploader scope")?
                        .0,
                )
            };
            uploader_route_step(steps, &native)?;
            require(
                if line.starts_with("python ") {
                    !all.contains(line)
                } else {
                    !actual_lines.contains(&line)
                },
                "original uploader launcher remains",
            )?;
            native[0].clone()
        } else {
            line.to_owned()
        };
        require(
            actual_lines
                .iter()
                .filter(|candidate| **candidate == line.as_str())
                .count()
                >= count,
            &format!("unrelated support command lost: {line}"),
        )?;
    }
    let native_startup = steps
        .iter()
        .filter(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.lines().any(|line| line.trim() == NATIVE_LOG_STARTUP))
        })
        .collect::<Vec<_>>();
    require(
        native_startup.len() == 1,
        "native logmessaged startup must run in exactly one step",
    )?;
    mandatory(native_startup[0])?;
    require(
        native_startup[0]["run"].as_str().is_some_and(|run| {
            run.lines()
                .next()
                .is_some_and(|line| line.trim() == "set -euo pipefail")
        }),
        "native logmessaged startup shell must fail closed",
    )?;
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
            NATIVE_LOG_STARTUP.into(),
            LOG_LANES.into(),
            LOG_ABSENT_ROOT.into(),
            LOG_CHECKER.into(),
            LOG_COLLECTOR.into(),
            LOG_PROOF.into(),
            LOG_ORIGINAL_ABSENT.into(),
            LOG_ORIGINAL_NOT_LINK.into(),
            LOG_COPIED_ABSENT.into(),
            LOG_COPIED_NOT_LINK.into(),
        ])
        .chain(NATIVE_UPLOADER.map(str::to_owned))
        .chain(NATIVE_UPLOADER_LOGGING.map(str::to_owned))
        .chain(
            SIBLING_ORIGINAL
                .iter()
                .flat_map(|(scope, _)| sibling_route(scope)),
        )
        .chain([UPLOADER_BUILD.to_owned()])
        .chain(UPLOADER_HTTP_PROVIDER.map(str::to_owned))
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
            && !step["run"].as_str().is_some_and(|run| {
                run.contains("rust/target/debug/check-timed-")
                    || run.contains(NATIVE_UPLOADER[0])
                    || run.contains(NATIVE_UPLOADER_LOGGING[0])
                    || run.contains("rust/target/debug/check-uploader-siblings --root")
            })
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

#[test]
fn uploader_routes_reject_same_step_duplicates_and_provider_rebinding() {
    let baseline = workflow();
    validate(&baseline).expect("actual uploader migration must pass before mutations");
    let routes = [
        NATIVE_UPLOADER.map(str::to_owned).to_vec(),
        NATIVE_UPLOADER_LOGGING.map(str::to_owned).to_vec(),
    ]
    .into_iter()
    .chain(
        SIBLING_ORIGINAL
            .iter()
            .map(|(scope, _)| sibling_route(scope)),
    );
    for native in routes {
        let (index, _) = uploader_route_step(
            baseline["jobs"]["support-runtime"]["steps"]
                .as_array()
                .unwrap(),
            &native,
        )
        .unwrap();
        let mut duplicate = baseline.clone();
        let run = duplicate["jobs"]["support-runtime"]["steps"][index]["run"]
            .as_str()
            .unwrap()
            .to_owned();
        duplicate["jobs"]["support-runtime"]["steps"][index]["run"] =
            Value::String(format!("{run}\n{}\n", native.join("\n")));
        assert!(
            validate(&duplicate).is_err(),
            "duplicate native block in one step admitted"
        );
        for softened in ["set +e", "if false; then"] {
            let mut changed = baseline.clone();
            let run = changed["jobs"]["support-runtime"]["steps"][index]["run"]
                .as_str()
                .unwrap()
                .to_owned();
            let run = if softened == "set +e" {
                run.replacen("set -euo pipefail", softened, 1)
            } else {
                format!(
                    "{}\nfi\n",
                    run.replacen(native[0].as_str(), &format!("{softened}\n{}", native[0]), 1)
                )
            };
            changed["jobs"]["support-runtime"]["steps"][index]["run"] = Value::String(run);
            assert!(
                validate(&changed).is_err(),
                "softened native uploader block admitted"
            );
        }
        for replacement in [
            "wrong-provider",
            "changed-fixture-or-hash",
            "changed-output-or-scope",
        ] {
            let mut changed = baseline.clone();
            let run = changed["jobs"]["support-runtime"]["steps"][index]["run"]
                .as_str()
                .unwrap()
                .to_owned();
            let marker = if replacement == "wrong-provider" {
                &native[1]
            } else if replacement == "changed-fixture-or-hash" {
                native.last().unwrap()
            } else {
                &native[0]
            };
            changed["jobs"]["support-runtime"]["steps"][index]["run"] =
                Value::String(run.replace(marker, replacement));
            assert!(
                validate(&changed).is_err(),
                "rebound native argument block admitted"
            );
        }
    }
    let mut late_build = baseline.clone();
    let steps = late_build["jobs"]["support-runtime"]["steps"]
        .as_array_mut()
        .unwrap();
    for step in steps.iter_mut() {
        if let Some(run) = step["run"].as_str() {
            step["run"] = Value::String(run.replace(UPLOADER_BUILD, ""));
        }
    }
    steps.push(serde_json::json!({"name":"late uploader build", "run":UPLOADER_BUILD}));
    assert!(
        validate(&late_build).is_err(),
        "late shared uploader build admitted"
    );
    let mut repeated_build = baseline.clone();
    let steps = repeated_build["jobs"]["support-runtime"]["steps"]
        .as_array_mut()
        .unwrap();
    let build = steps
        .iter_mut()
        .find(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.lines().any(|line| line.trim() == UPLOADER_BUILD))
        })
        .unwrap();
    build["run"] = Value::String(format!(
        "{}\n{UPLOADER_BUILD}\n",
        build["run"].as_str().unwrap()
    ));
    assert!(
        validate(&repeated_build).is_err(),
        "duplicate shared uploader build admitted"
    );
}
