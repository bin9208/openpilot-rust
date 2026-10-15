//! Original eight synchronization guard cases, plus real CLI/fake-gh boundaries.
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    upstream_sync,
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
};
fn a() -> Value {
    json!("a".repeat(40))
}
fn b() -> Value {
    json!("b".repeat(40))
}
fn comparison() -> Value {
    json!({"status":"ahead","behind_by":0,"ahead_by":1,"base_commit":{"sha":a()},"merge_base_commit":{"sha":a()}})
}
fn should_update(current: &Value, upstream: &Value, comparison: &Value) -> Result<bool> {
    upstream_sync::should_update(current, upstream, &comparison.to_string())
}
#[test]
fn identical_is_noop() {
    assert!(!should_update(&a(), &a(), &json!({})).unwrap());
}
#[test]
fn fast_forward_is_allowed() {
    assert!(should_update(&a(), &b(), &comparison()).unwrap());
}
#[test]
fn divergent_or_rewound_upstream_is_rejected() {
    for status in ["diverged", "behind", "identical"] {
        let mut c = comparison();
        c["status"] = json!(status);
        assert!(should_update(&a(), &b(), &c).is_err(), "{status}");
    }
}
#[test]
fn racing_or_unrelated_comparison_is_rejected() {
    for (key, value) in [
        ("base_commit", json!({"sha":b()})),
        ("merge_base_commit", json!({"sha":b()})),
        ("behind_by", json!(1)),
        ("ahead_by", json!(0)),
    ] {
        let mut c = comparison();
        c[key] = value;
        assert!(should_update(&a(), &b(), &c).is_err(), "{key}");
    }
}
#[test]
fn malformed_sha_is_rejected() {
    for value in [
        json!(""),
        json!("main"),
        Value::Null,
        json!("a".repeat(39)),
        json!("A".repeat(40)),
        json!(1),
    ] {
        assert!(
            should_update(&value, &b(), &comparison()).is_err(),
            "{value}"
        );
        assert!(
            should_update(&a(), &value, &comparison()).is_err(),
            "{value}"
        );
    }
}
#[test]
fn original_boolean_integer_and_float_semantics_are_preserved() {
    #[derive(Debug, serde::Deserialize)]
    struct Oracle {
        current: Value,
        upstream: Value,
        comparison: Box<serde_json::value::RawValue>,
        outcome: Value,
    }
    let cases: Vec<Oracle> = serde_json::from_str(include_str!(
        "fixtures/upstream-sync/original-number-semantics.json"
    ))
    .unwrap();
    for case in cases {
        let result =
            upstream_sync::should_update(&case.current, &case.upstream, case.comparison.get());
        match case.outcome.as_bool() {
            Some(value) => assert_eq!(result.unwrap(), value, "{case:?}"),
            None => assert!(result.is_err(), "{case:?}"),
        };
    }
}
struct Peer {
    calls: Vec<Vec<String>>,
    responses: Vec<Value>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert_eq!(spec.timeout.as_secs(), 60);
        self.calls.push(spec.argv.clone());
        let value = self.responses.remove(0);
        Ok(Captured {
            code: 0,
            stdout: serde_json::to_vec(&value).unwrap(),
            stderr: vec![],
        })
    }
}
fn responses() -> Vec<Value> {
    vec![
        json!({"parent":{"full_name":"ajouatom/openpilot"}}),
        json!({"object":{"sha":a()}}),
        json!({"object":{"sha":b()}}),
        comparison(),
        json!({}),
        json!({"object":{"sha":b()}}),
    ]
}
#[test]
fn other_repository_cannot_mutate() {
    let mut peer = Peer {
        calls: vec![],
        responses: vec![],
    };
    assert!(upstream_sync::sync(Some("someone/other"), None, &mut peer).is_err());
    assert!(peer.calls.is_empty());
}
#[test]
fn only_expected_ref_is_updated_without_force() {
    let mut peer = Peer {
        calls: vec![],
        responses: responses(),
    };
    let report = upstream_sync::sync(Some("bin9208/openpilot"), None, &mut peer).unwrap();
    assert_eq!(
        peer.calls[4],
        vec![
            "gh",
            "api",
            "repos/bin9208/openpilot/git/refs/heads/carrot-wip",
            "--method",
            "PATCH",
            "-f",
            &format!("sha={}", "b".repeat(40)),
            "-F",
            "force=false"
        ]
    );
    assert_eq!(
        serde_json::to_value(report).unwrap(),
        json!({"repository":"bin9208/openpilot","branch":"carrot-wip","before":a(),"after":b(),"changed":true,"force":false})
    );
}
#[test]
fn wrong_parent_is_rejected_before_reading_refs() {
    let mut peer = Peer {
        calls: vec![],
        responses: vec![json!({"parent":{"full_name":"other/repo"}})],
    };
    assert!(upstream_sync::sync(Some("bin9208/openpilot"), None, &mut peer).is_err());
    assert_eq!(peer.calls.len(), 1);
}
struct Fixture {
    _temp: tempfile::TempDir,
    path: PathBuf,
}
impl Fixture {
    fn new(values: &[Value]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_owned();
        fs::create_dir(path.join("calls")).unwrap();
        for (i, v) in values.iter().enumerate() {
            fs::write(
                path.join(format!("{}.json", i + 1)),
                serde_json::to_vec(v).unwrap(),
            )
            .unwrap();
        }
        let gh = path.join("gh");
        fs::write(
            &gh,
            r#"#!/bin/bash
set -eu
n=0
if [ -f "$FAKE_GH_ROOT/count" ]; then n=$(cat "$FAKE_GH_ROOT/count"); fi
n=$((n+1))
printf '%s\n' "$n" > "$FAKE_GH_ROOT/count"
printf '%s\n' "$@" > "$FAKE_GH_ROOT/calls/$n"
if [ -f "$FAKE_GH_ROOT/$n.exit" ]; then exit "$(cat "$FAKE_GH_ROOT/$n.exit")"; fi
cat "$FAKE_GH_ROOT/$n.json"
"#,
        )
        .unwrap();
        fs::set_permissions(gh, fs::Permissions::from_mode(0o755)).unwrap();
        Self { _temp: temp, path }
    }
    fn run(&self, repo: Option<&str>, summary: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sync-upstream"));
        command
            .env("PATH", format!("{}:/usr/bin:/bin", self.path.display()))
            .env("FAKE_GH_ROOT", &self.path)
            .env_remove("GITHUB_REPOSITORY")
            .env_remove("GITHUB_STEP_SUMMARY")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN");
        if let Some(repo) = repo {
            command.env("GITHUB_REPOSITORY", repo);
        }
        if summary {
            command.env("GITHUB_STEP_SUMMARY", self.path.join("summary"));
        }
        command.output().unwrap()
    }
    fn call(&self, n: usize) -> Vec<String> {
        fs::read_to_string(self.path.join(format!("calls/{n}")))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn count(&self) -> usize {
        fs::read_to_string(self.path.join("count"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap_or(0)
    }
}
#[test]
fn actual_cli_patches_exact_ref_reads_back_and_appends_summary() {
    let f = Fixture::new(&responses());
    fs::write(f.path.join("summary"), "existing\n").unwrap();
    let output = f.run(Some("bin9208/openpilot"), true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report,
        json!({"repository":"bin9208/openpilot","branch":"carrot-wip","before":a(),"after":b(),"changed":true,"force":false})
    );
    assert_eq!(f.count(), 6);
    assert_eq!(
        f.call(5),
        vec![
            "api",
            "repos/bin9208/openpilot/git/refs/heads/carrot-wip",
            "--method",
            "PATCH",
            "-f",
            &format!("sha={}", "b".repeat(40)),
            "-F",
            "force=false"
        ]
    );
    assert_eq!(
        f.call(6),
        vec!["api", "repos/bin9208/openpilot/git/ref/heads/carrot-wip"]
    );
    let expected = format!(
        "existing\nFast-forward-only mirror result:\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&report).unwrap()
    );
    let actual = fs::read_to_string(f.path.join("summary")).unwrap();
    let json_start = actual.find("```json\n").unwrap() + 8;
    let json_end = actual[json_start..].find("\n```").unwrap() + json_start;
    assert_eq!(
        serde_json::from_str::<Value>(&actual[json_start..json_end]).unwrap(),
        report
    );
    assert!(actual.starts_with("existing\nFast-forward-only mirror result:\n\n```json\n"));
    assert!(actual.ends_with("\n```\n"));
    assert_eq!(expected.len(), actual.len());
}
#[test]
fn actual_cli_wrong_or_missing_repository_never_calls_gh() {
    for repo in [None, Some("bin9208/openpilot-rust"), Some("someone/other")] {
        let f = Fixture::new(&[]);
        let output = f.run(repo, true);
        assert!(!output.status.success());
        assert_eq!(f.count(), 0);
        assert!(output.stdout.is_empty());
        assert!(!f.path.join("summary").exists());
    }
}
#[test]
fn actual_cli_identical_is_noop_with_final_readback() {
    let f = Fixture::new(&[
        json!({"parent":{"full_name":"ajouatom/openpilot"}}),
        json!({"object":{"sha":a()}}),
        json!({"object":{"sha":a()}}),
        json!({"object":{"sha":a()}}),
    ]);
    let output = f.run(Some("bin9208/openpilot"), false);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["changed"], false);
    assert_eq!(f.count(), 4);
    for n in 1..=4 {
        assert!(!f.call(n).iter().any(|v| v == "PATCH"));
    }
}
#[test]
fn actual_cli_bad_parent_comparison_readback_or_gh_response_fails_closed() {
    for mode in [
        "parent",
        "comparison",
        "readback",
        "api-failure",
        "invalid-json",
        "bad-current-sha",
    ] {
        let mut values = responses();
        match mode {
            "parent" => values[0] = json!({"parent":{"full_name":"other/repo"}}),
            "comparison" => values[3]["status"] = json!("diverged"),
            "readback" => values[5] = json!({"object":{"sha":a()}}),
            "bad-current-sha" => values[1] = json!({"object":{"sha":"main"}}),
            _ => {}
        }
        let f = Fixture::new(&values);
        if mode == "api-failure" {
            fs::write(f.path.join("1.exit"), "9").unwrap();
        }
        if mode == "invalid-json" {
            fs::write(f.path.join("1.json"), "invalid JSON").unwrap();
        }
        let output = f.run(Some("bin9208/openpilot"), true);
        assert!(!output.status.success(), "{mode}");
        assert!(output.stdout.is_empty(), "{mode}");
        assert!(!f.path.join("summary").exists(), "{mode}");
        if mode != "readback" {
            for n in 1..=f.count() {
                assert!(!f.call(n).iter().any(|v| v == "PATCH"), "{mode}");
            }
        }
    }
}
#[test]
fn report_order_matches_original_serialized_struct_fields() {
    let mut peer = Peer {
        calls: vec![],
        responses: responses(),
    };
    let report = upstream_sync::sync(Some("bin9208/openpilot"), None, &mut peer).unwrap();
    let text = serde_json::to_string(&report).unwrap();
    let keys = [
        "repository",
        "branch",
        "before",
        "after",
        "changed",
        "force",
    ];
    let positions = keys.map(|k| text.find(&format!("\"{k}\":")).unwrap());
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
}
#[test]
fn actual_cli_keeps_raw_big_integer_and_float_comparison_tokens() {
    let huge = format!("1{}", "0".repeat(80));
    for (token, accepted) in [
        (huge.clone(), true),
        (format!("{huge}.0"), false),
        ("true".into(), true),
        ("false".into(), false),
    ] {
        let fixture = Fixture::new(&responses());
        let raw = comparison().to_string().replacen(
            "\"ahead_by\":1",
            &format!("\"ahead_by\":{token}"),
            1,
        );
        fs::write(fixture.path.join("4.json"), raw).unwrap();
        let output = fixture.run(Some("bin9208/openpilot"), false);
        assert_eq!(
            output.status.success(),
            accepted,
            "token {token}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fixture.count(), if accepted { 6 } else { 4 });
    }
}
#[test]
fn actual_cli_summary_write_failure_keeps_the_verified_stdout_report() {
    let fixture = Fixture::new(&responses());
    fs::create_dir(fixture.path.join("summary")).unwrap();
    let output = fixture.run(Some("bin9208/openpilot"), true);
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["after"], b());
    assert_eq!(fixture.count(), 6);
}
