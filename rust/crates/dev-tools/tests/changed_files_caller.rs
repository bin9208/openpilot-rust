//! Maintained CI caller regression after accepted codec/lifecycle gates.
#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
use openpilot_dev_tools::ci_policy::{parse, TRUSTED_SOURCE};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

#[path = "changed_files_caller/absence.rs"]
mod absence;

const REGRESSION: &str = "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --no-default-features --features changed-files-native --test changed_files_caller";
const RUN: &str = "set -euo pipefail\nOPENPILOT_CI_PREPARED_RUST_ROOT=\"$CI_PREPARED_RUST_ROOT\" OPENPILOT_CI_PREPARED_RUST_MANIFEST=\"$CI_PREPARED_RUST_MANIFEST\" \\\n  \"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 6144\ncargo run --quiet --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --no-default-features --features changed-files-native --bin check-changed-files --\n";
fn caller_policy(workflow: &Value) -> bool {
    let Some(steps) = workflow["jobs"]["python-android"]["steps"].as_array() else {
        return false;
    };
    let selected: Vec<_> = steps
        .iter()
        .filter(|s| s["name"] == "Check changed Python syntax and whitespace")
        .collect();
    if selected.len() != 1 {
        return false;
    }
    let step = selected[0];
    if step["run"] != RUN
        || step["env"]["BASE_SHA"]
            != "${{ github.event.pull_request.base.sha || github.event.before }}"
        || step["if"] != TRUSTED_SOURCE
        || step["working-directory"] != "${{ env.DIAG_SOURCE }}"
        || step.get("continue-on-error").is_some()
    {
        return false;
    }
    let Some(native) = workflow["jobs"]["native-routing"]["steps"].as_array() else {
        return false;
    };
    let regressions: Vec<_> = native
        .iter()
        .filter(|s| {
            s["run"]
                .as_str()
                .is_some_and(|run| run.lines().any(|line| line == REGRESSION))
        })
        .collect();
    if regressions.len() != 1
        || regressions[0]["if"] != TRUSTED_SOURCE
        || regressions[0]["working-directory"] != "${{ env.DIAG_SOURCE }}"
        || regressions[0].get("continue-on-error").is_some()
    {
        return false;
    }
    let commands = native
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    commands.lines().filter(|s| *s == REGRESSION).count() == 1
        && !commands.contains("unittest discover -s tools/ci/tests")
        && commands.contains("\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 6144")
}
#[test]
fn maintained_caller_and_native_initial_push_regression_are_mandatory() {
    let _absence = absence::Guard::current(
        "maintained_caller_and_native_initial_push_regression_are_mandatory",
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow =
        parse(&fs::read_to_string(root.join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    assert!(caller_policy(&workflow));
    let index = workflow["jobs"]["python-android"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["name"] == "Check changed Python syntax and whitespace")
        .unwrap();
    for mutation in [
        "base",
        "guard",
        "python",
        "soft-fail",
        "skip",
        "private-path",
        "fork",
        "regression",
    ] {
        let mut changed = workflow.clone();
        let step = &mut changed["jobs"]["python-android"]["steps"][index];
        match mutation {
            "base" => step["env"]["BASE_SHA"] = "HEAD^".into(),
            "guard" => {
                step["run"] = RUN
                    .lines()
                    .filter(|l| !l.contains("check-disk-space"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into()
            }
            "python" => step["run"] = "python tools/ci/check_changed_files.py".into(),
            "soft-fail" => step["continue-on-error"] = "true".into(),
            "skip" => step["if"] = "false".into(),
            "private-path" => step["working-directory"] = "${{ github.workspace }}".into(),
            "fork" => step["if"] = "true".into(),
            "regression" => {
                for step in changed["jobs"]["native-routing"]["steps"]
                    .as_array_mut()
                    .unwrap()
                {
                    if let Some(run) = step["run"].as_str() {
                        step["run"] = run.replace(REGRESSION, "true").into();
                    }
                }
            }
            _ => unreachable!(),
        }
        assert!(!caller_policy(&changed), "accepted {mutation}");
    }
}
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn commit(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(root, &["commit", "-m", message]);
}
fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init"]);
    git(root.path(), &["config", "user.name", "CI test"]);
    git(root.path(), &["config", "user.email", "ci@example.invalid"]);
    fs::write(root.path().join("base.txt"), b"base\n").unwrap();
    commit(root.path(), "base");
    git(
        root.path(),
        &["update-ref", "refs/remotes/origin/dev", "HEAD"],
    );
    root
}
fn check(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_check-changed-files"))
        .current_dir(root)
        .env("BASE_SHA", "0".repeat(40))
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap()
}
#[test]
fn initial_push_checks_earlier_commits() {
    let _absence = absence::Guard::current("initial_push_checks_earlier_commits");
    // Preserve the original sole unittest's multi-commit initial-push assertion.
    let root = repository();
    fs::write(root.path().join("broken.py"), b"def broken(:\n").unwrap();
    commit(root.path(), "earlier syntax error");
    fs::write(root.path().join("last.txt"), b"last\n").unwrap();
    commit(root.path(), "later unrelated change");
    let output = check(root.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Syntax"));
    assert!(output.stdout.is_empty());
}
#[test]
fn changed_current_worktree_bytes_are_checked_after_head_selection() {
    let _absence =
        absence::Guard::current("changed_current_worktree_bytes_are_checked_after_head_selection");
    let root = repository();
    fs::write(root.path().join("changed.py"), b"x=1\n").unwrap();
    commit(root.path(), "valid HEAD source");
    assert!(check(root.path()).status.success());
    fs::write(root.path().join("changed.py"), b"def broken(:\n").unwrap();
    let output = check(root.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Syntax"));
}
#[test]
fn whitespace_precedes_syntax_rejection() {
    let _absence = absence::Guard::current("whitespace_precedes_syntax_rejection");
    let root = repository();
    fs::write(root.path().join("broken.py"), b"def broken(:\n").unwrap();
    fs::write(root.path().join("space.txt"), b"trailing \n").unwrap();
    commit(root.path(), "whitespace and syntax errors");
    let output = check(root.path());
    assert!(!output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("Whitespace"));
    assert!(!diagnostic.contains("Syntax"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("trailing whitespace"));
}
