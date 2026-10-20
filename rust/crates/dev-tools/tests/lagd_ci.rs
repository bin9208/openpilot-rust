//! Keep the complete six-checker native closure and every unrelated estimator gate.
use openpilot_dev_tools::ci_policy::parse;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const TOOLS: [&str; 6] = ["numeric", "loop", "cache", "packet", "daemon", "params-io"];
const TESTS: [&str; 15] = [
    "lagd_reference",
    "lagd_reference_generic",
    "lagd_frames",
    "lagd_packet_reference",
    "lagd_loop_reference",
    "lagd_random",
    "lagd_numeric_cases",
    "lagd_check",
    "lagd_cli",
    "lagd_live_check",
    "lagd_live_contracts",
    "lagd_live_native",
    "lagd_session",
    "lagd_live_cli",
    "lagd_retirement",
];
const BUILD: &str = "Build and test native lagd QA tools";
const RUN: &str = "Compare delay numerical policy, full source loop, cache and native IPC";
const ESTIMATORS: &str = "Build native estimators and unchanged source oracle";
const EIGEN: &str = "Require native Eigen header staging against sealed original install";
const OLD_EIGEN_INSTALL: &str = "python -m pip install 'git+https://github.com/commaai/dependencies.git@40e5d76de1b33a86c5181b63db6782d8f06da1da#subdirectory=eigen'";
const OLD_EIGEN_IMPORT: &str = "python -c 'import eigen; print(\"LOCATIOND_EIGEN_INCLUDE=\" + eigen.INCLUDE_DIR); print(\"PARAMSD_EIGEN_INCLUDE=\" + eigen.INCLUDE_DIR)' >> \"$GITHUB_ENV\"";
const NATIVE_EIGEN_BUILD: &str = "cargo build --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features eigen-tools-native --bin stage-eigen-headers";
const NATIVE_EIGEN_STAGE: &str = "\"$stager\" --wheel \"$wheel\" --output \"$proof/$architecture-native\" --architecture \"$architecture\" > \"$proof/$architecture-stage.json\"";
const NATIVE_EIGEN_EXPORT: &str = "printf 'LOCATIOND_EIGEN_INCLUDE=%s/x86_64-native/install\\nPARAMSD_EIGEN_INCLUDE=%s/x86_64-native/install\\n' \"$proof\" \"$proof\" >> \"$GITHUB_ENV\"";
const GUARD: &str =
    "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib ";
fn original() -> Value {
    let raw = include_bytes!("fixtures/lagd-tools/estimation-ci-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "cc600cdac529e4ba8b0bfc615e989dd43bf665e5aa94785816c73810d28fd206"
    );
    serde_json::from_slice(raw).unwrap()
}
fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn mandatory(value: &Value) -> Result<(), String> {
    require(
        value.get("if").is_none() && value.get("continue-on-error").is_none(),
        "lagd gate became conditional/optional",
    )
}
fn command(tool: &str) -> String {
    let (binary, output) = match tool {
        "numeric" => ("examples/lag_numeric", "numeric.json"),
        "loop" => ("examples/lag_loop", "loop"),
        "cache" => ("examples/lag_cache", "cache.json"),
        "packet" => ("examples/lag_packet", "packet.json"),
        "daemon" => ("openpilot-lagd", "daemon"),
        _ => ("openpilot-lagd", "params-io.json"),
    };
    format!("rust/target/debug/check-lagd-{tool} \"$PWD/rust/target/debug/{binary}\"{} \"$RUNNER_TEMP/estimation/lagd/{output}\"",if tool=="daemon"{" \"$PWD/rust/target/debug/examples/lag_ipc_peer\""}else{""})
}
fn lines(steps: &[Value]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for step in steps {
        if let Some(run) = step["run"].as_str() {
            for line in run.lines().map(str::trim).filter(|line| !line.is_empty()) {
                *out.entry(line.to_owned()).or_default() += 1;
            }
        }
    }
    out
}
fn validate(workflow: &Value) -> Result<(), String> {
    let job = &workflow["jobs"]["estimation-runtime"];
    mandatory(job)?;
    let baseline = original();
    let old = &baseline["job"];
    for key in ["name", "runs-on", "timeout-minutes", "env"] {
        require(
            job[key] == old[key],
            "unrelated estimation job metadata/environment changed",
        )?;
    }
    let steps = job["steps"].as_array().ok_or("missing estimation steps")?;
    let eigen = steps
        .iter()
        .position(|step| step["name"] == EIGEN)
        .ok_or("native Eigen staging missing")?;
    require(
        steps.iter().filter(|step| step["name"] == EIGEN).count() == 1,
        "native Eigen staging duplicated",
    )?;
    mandatory(&steps[eigen])?;
    let eigen_script = steps[eigen]["run"]
        .as_str()
        .ok_or("native Eigen staging commands missing")?;
    let eigen_lines = eigen_script.lines().map(str::trim).collect::<Vec<_>>();
    for line in [NATIVE_EIGEN_BUILD, NATIVE_EIGEN_STAGE, NATIVE_EIGEN_EXPORT] {
        require(
            eigen_lines
                .iter()
                .filter(|current| **current == line)
                .count()
                == 1,
            "native Eigen build or estimator include export changed",
        )?;
    }
    require(
        eigen_lines.contains(&"for item in x86_64:linux_x86_64 aarch64:linux_aarch64 macos-arm64:macosx_11_0_arm64; do"),
        "native Eigen header staging lost the pinned platform matrix",
    )?;
    let estimators = steps
        .iter()
        .position(|step| step["name"] == ESTIMATORS)
        .ok_or("estimator build missing")?;
    require(
        eigen < estimators,
        "native Eigen staging moved after estimator build",
    )?;
    for oldstep in old["steps"].as_array().unwrap() {
        if oldstep.get("uses").is_some() {
            require(
                steps.iter().any(|step| step == oldstep),
                "original estimator action changed",
            )?;
        }
    }
    let build = steps
        .iter()
        .position(|step| step["name"] == BUILD)
        .ok_or("native lagd build missing")?;
    mandatory(&steps[build])?;
    require(
        eigen < build,
        "native Eigen staging moved after estimator build",
    )?;
    let build_script = steps[build]["run"]
        .as_str()
        .ok_or("native lagd build commands missing")?;
    let build_lines = build_script
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    for prefix in ["cargo build ", "cargo test ", "cargo clippy "] {
        let selected = build_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with(prefix))
            .collect::<Vec<_>>();
        require(
            selected.len() == 1,
            "missing or duplicate native lagd Cargo gate",
        )?;
        let (index, command) = selected[0];
        require(
            command.contains("-p openpilot-dev-tools")
                && command.contains("--features lagd-tools-native")
                && command.contains("--locked"),
            "lagd feature/package/lock missing",
        )?;
        require(
            index > 0 && build_lines[index - 1].starts_with(GUARD),
            "fresh lagd Cargo reserve missing",
        )?;
        if prefix != "cargo test " {
            for tool in TOOLS {
                require(
                    command.matches(&format!("--bin check-lagd-{tool}")).count() == 1,
                    "missing native lagd binary",
                )?;
            }
        }
        if prefix != "cargo build " {
            for test in TESTS {
                require(
                    command
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .windows(2)
                        .filter(|v| *v == ["--test", test])
                        .count()
                        == 1,
                    "missing native lagd regression suite",
                )?;
            }
        }
        if prefix == "cargo clippy " {
            require(
                command.contains("--lib") && command.ends_with("-- -D warnings"),
                "strict lagd Clippy missing",
            )?;
        }
    }
    let install = build_lines
        .iter()
        .position(|line| *line == "rustup component add clippy --toolchain 1.94.0")
        .ok_or("lagd Clippy component not installed")?;
    require(
        install > 0 && build_lines[install - 1].starts_with(GUARD),
        "Clippy installation reserve missing",
    )?;
    let run = steps
        .iter()
        .position(|step| step["name"] == RUN)
        .ok_or("six actual lagd comparisons missing")?;
    mandatory(&steps[run])?;
    require(build < run, "lagd native build occurs after its consumers")?;
    let commands = steps[run]["run"]
        .as_str()
        .ok_or("missing live lagd commands")?
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    for tool in TOOLS {
        let cmd = command(tool);
        let index = commands
            .iter()
            .position(|line| *line == cmd)
            .ok_or("exact native lagd caller missing")?;
        require(
            commands.iter().filter(|line| **line == cmd).count() == 1,
            "duplicate lagd caller",
        )?;
        require(
            index > 0 && commands[index - 1].starts_with(GUARD),
            "actual lagd output reserve missing",
        )?;
    }
    let current = lines(steps);
    let original = lines(old["steps"].as_array().unwrap());
    for (line, count) in original {
        if !line.starts_with("python rust/tools/check_lagd_")
            && line != OLD_EIGEN_INSTALL
            && line != OLD_EIGEN_IMPORT
        {
            require(
                current.get(&line) == Some(&count),
                "unrelated estimator command removed/duplicated/changed",
            )?;
        }
    }
    for line in [OLD_EIGEN_INSTALL, OLD_EIGEN_IMPORT] {
        require(
            current.get(line).is_none(),
            "retired Eigen pip/import caller reappeared",
        )?;
    }
    for step in steps {
        if let Some(script) = step["run"].as_str() {
            require(
                !script.contains("check_lagd_")
                    && !script.contains("lagd_reference.py")
                    && !script.contains("lagd_loop_reference.py")
                    && !script.contains("lagd_frames.py"),
                "retired lagd Python executable route remains",
            )?;
            if step["name"] == BUILD || step["name"] == RUN {
                require(
                    !script.contains("|| true")
                        && !script.contains("set +e")
                        && !script.contains("continue-on-error"),
                    "native lagd failure was softened",
                )?;
            }
        }
    }
    Ok(())
}
fn workflow() -> Value {
    parse(include_str!("../../../../.github/workflows/rust.yml")).unwrap()
}
#[test]
fn six_native_lagd_checkers_full_references_and_existing_estimators_are_mandatory() {
    validate(&workflow()).unwrap();
}
#[test]
fn missing_native_tools_tests_guards_or_unrelated_gates_cannot_pass() {
    let current = workflow();
    validate(&current).unwrap();
    let steps = current["jobs"]["estimation-runtime"]["steps"]
        .as_array()
        .unwrap();
    let build = steps.iter().position(|step| step["name"] == BUILD).unwrap();
    let run = steps.iter().position(|step| step["name"] == RUN).unwrap();
    let eigen = steps.iter().position(|step| step["name"] == EIGEN).unwrap();
    let estimators = steps
        .iter()
        .position(|step| step["name"] == ESTIMATORS)
        .unwrap();
    let mut changed = current.clone();
    let moved = changed["jobs"]["estimation-runtime"]["steps"]
        .as_array_mut()
        .unwrap()
        .remove(eigen);
    changed["jobs"]["estimation-runtime"]["steps"]
        .as_array_mut()
        .unwrap()
        .insert(estimators, moved);
    assert!(
        validate(&changed).is_err(),
        "accepted native Eigen staging after estimator build"
    );
    for marker in [NATIVE_EIGEN_BUILD, NATIVE_EIGEN_STAGE, NATIVE_EIGEN_EXPORT] {
        let mut changed = current.clone();
        let script = changed["jobs"]["estimation-runtime"]["steps"][eigen]["run"]
            .as_str()
            .unwrap()
            .replace(marker, "removed-by-regression");
        changed["jobs"]["estimation-runtime"]["steps"][eigen]["run"] = script.into();
        assert!(validate(&changed).is_err(), "accepted missing {marker}");
    }
    let mut changed = current.clone();
    changed["jobs"]["estimation-runtime"]["steps"][eigen]["if"] = "always()".into();
    assert!(
        validate(&changed).is_err(),
        "accepted optional native Eigen stage"
    );
    let mut changed = current.clone();
    let script = changed["jobs"]["estimation-runtime"]["steps"][eigen]["run"]
        .as_str()
        .unwrap()
        .to_owned();
    changed["jobs"]["estimation-runtime"]["steps"][eigen]["run"] =
        format!("{script}\n{OLD_EIGEN_INSTALL}\n").into();
    assert!(
        validate(&changed).is_err(),
        "accepted restored Eigen pip caller"
    );
    for marker in TOOLS
        .map(|v| format!("--bin check-lagd-{v}"))
        .into_iter()
        .chain(TESTS.map(|v| format!("--test {v}")).into_iter())
        .chain([
            "--features lagd-tools-native".into(),
            "--locked".into(),
            "-- -D warnings".into(),
            "rustup component add clippy --toolchain 1.94.0".into(),
        ])
    {
        let mut changed = current.clone();
        let script = changed["jobs"]["estimation-runtime"]["steps"][build]["run"]
            .as_str()
            .unwrap()
            .replace(&marker, "removed-by-regression");
        changed["jobs"]["estimation-runtime"]["steps"][build]["run"] = script.into();
        assert!(validate(&changed).is_err(), "accepted missing {marker}");
    }
    for tool in TOOLS {
        let mut changed = current.clone();
        let script = changed["jobs"]["estimation-runtime"]["steps"][run]["run"]
            .as_str()
            .unwrap()
            .replace(&command(tool), "removed-by-regression");
        changed["jobs"]["estimation-runtime"]["steps"][run]["run"] = script.into();
        assert!(validate(&changed).is_err());
    }
    for index in [build, run] {
        for key in ["if", "continue-on-error"] {
            let mut changed = current.clone();
            changed["jobs"]["estimation-runtime"]["steps"][index][key] = "always()".into();
            assert!(validate(&changed).is_err());
        }
        let mut changed = current.clone();
        let script = changed["jobs"]["estimation-runtime"]["steps"][index]["run"]
            .as_str()
            .unwrap()
            .replace(GUARD, "removed-reserve ");
        changed["jobs"]["estimation-runtime"]["steps"][index]["run"] = script.into();
        assert!(validate(&changed).is_err());
    }
    let mut changed = current.clone();
    let native = changed["jobs"]["estimation-runtime"]["steps"]
        .as_array_mut()
        .unwrap()
        .remove(build);
    changed["jobs"]["estimation-runtime"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(native);
    assert!(validate(&changed).is_err());
    let mut changed = current.clone();
    for step in changed["jobs"]["estimation-runtime"]["steps"]
        .as_array_mut()
        .unwrap()
    {
        if let Some(script) = step["run"].as_str() {
            if script.contains("-fsanitize=address,undefined") {
                step["run"] = script
                    .replace("-fsanitize=address,undefined", "removed-sanitizer")
                    .into();
                break;
            }
        }
    }
    assert!(validate(&changed).is_err());
}
