//! Native counterparts of tools/ci/tests/test_workflow_policy.py.
use openpilot_dev_tools::ci_policy::{check_workflow_policy, parse, run_shell, validate_fast_gate};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn inputs() -> Vec<(String, String)> {
    [
        "fast-checks.yml",
        "integration.yml",
        "user-docs.yaml",
        "tests.yaml",
        "naver-ci.yml",
        "naver-apk-tools.yml",
        "jetlink.yml",
    ]
    .into_iter()
    .filter_map(|name| {
        let path = root().join(".github/workflows").join(name);
        path.exists()
            .then(|| (name.to_owned(), fs::read_to_string(path).unwrap()))
    })
    .collect()
}
#[test]
fn fast_checks_cover_all_branch_pushes_and_protected_prs() {
    let files = inputs();
    check_workflow_policy(&files).unwrap();
}
#[test]
fn protected_workflow_mutations_are_rejected() {
    let original = inputs();
    for (name, from, to) in [
        ("fast-checks.yml", "branches: ['**']", "branches: [dev]"),
        (
            "fast-checks.yml",
            "branches: [dev, main]",
            "branches: [main]",
        ),
        (
            "fast-checks.yml",
            "branches: ['**']",
            "branches: ['**']\n    paths: ['rust/**']",
        ),
        ("integration.yml", "${{ always() }}", "${{ success() }}"),
        ("integration.yml", "test \"$BUILD\" = success", "true"),
        ("user-docs.yaml", "- dev", "- other"),
    ] {
        let mut files = original.clone();
        let source = &mut files.iter_mut().find(|(file, _)| file == name).unwrap().1;
        assert!(
            source.contains(from),
            "mutation anchor absent: {name} {from}"
        );
        *source = source.replacen(from, to, 1);
        assert!(
            check_workflow_policy(&files).is_err(),
            "accepted {name}: {from} -> {to}"
        );
    }
}
#[test]
fn yaml_scalars_preserve_github_on_and_base_loader_values() {
    let value =
        parse("on: {push: {branches: ['**']}}\nnumber: 1\nflag: false\nquoted: '01'\n").unwrap();
    assert_eq!(value["on"]["push"]["branches"][0], "**");
    assert_eq!(value["number"], "1");
    assert_eq!(value["flag"], "false");
    assert_eq!(value["quoted"], "01");
}
#[test]
fn yaml_raw_scalars_anchors_and_multiline_match_base_loader() {
    let value = parse("on: true\nleading: 001\nhex: 0x1\nnull: null\nempty:\nquoted: '01'\ntext: |\n  first\n  second\nanchor: &settings {channel: 001, flag: true}\nalias: *settings\n").unwrap();
    assert_eq!(value["on"], "true");
    assert_eq!(value["leading"], "001");
    assert_eq!(value["hex"], "0x1");
    assert_eq!(value["null"], "null");
    assert_eq!(value["empty"], "");
    assert_eq!(value["quoted"], "01");
    assert_eq!(value["text"], "first\nsecond\n");
    assert_eq!(
        value["alias"],
        serde_json::json!({"channel":"001", "flag":"true"})
    );
}

#[test]
fn parallel_fast_gate_requires_every_worker_and_actual_success() {
    let original =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_fast_gate(&original).unwrap();
    let gate = &original["jobs"]["fast"];
    let script = gate["steps"][0]["run"].as_str().unwrap();
    let success = BTreeMap::from([
        ("NATIVE".into(), "success".into()),
        ("PYTHON_ANDROID".into(), "success".into()),
    ]);
    assert!(run_shell(script, &success).unwrap().success());
    for name in ["NATIVE", "PYTHON_ANDROID"] {
        for state in [
            None,
            Some(""),
            Some("failure"),
            Some("cancelled"),
            Some("skipped"),
            Some("timed_out"),
        ] {
            let mut env = success.clone();
            if let Some(state) = state {
                env.insert(name.into(), state.into());
            } else {
                env.remove(name);
            }
            assert!(
                !run_shell(script, &env).unwrap().success(),
                "accepted {name}={state:?}"
            );
        }
    }
    for dependency in ["native", "python-android"] {
        let mut missing = original.clone();
        missing["jobs"].as_object_mut().unwrap().remove(dependency);
        assert!(validate_fast_gate(&missing).is_err());
        let mut optional = original.clone();
        optional["jobs"][dependency]["if"] = "false".into();
        assert!(validate_fast_gate(&optional).is_err());
        let mut ignored = original.clone();
        ignored["jobs"][dependency]["continue-on-error"] = "true".into();
        assert!(validate_fast_gate(&ignored).is_err());
        let mut sequential = original.clone();
        sequential["jobs"][dependency]["needs"] = serde_json::json!(["other"]);
        assert!(validate_fast_gate(&sequential).is_err());
        let mut skipped_step = original.clone();
        skipped_step["jobs"][dependency]["steps"][0]["if"] = "false".into();
        assert!(validate_fast_gate(&skipped_step).is_err());
        let mut ignored_step = original.clone();
        ignored_step["jobs"][dependency]["steps"][0]["continue-on-error"] = "true".into();
        assert!(validate_fast_gate(&ignored_step).is_err());
    }
    for name in ["NATIVE", "PYTHON_ANDROID"] {
        let mut missing = original.clone();
        missing["jobs"]["fast"]["steps"][0]["env"]
            .as_object_mut()
            .unwrap()
            .remove(name);
        assert!(validate_fast_gate(&missing).is_err());
        let mut wrong = original.clone();
        wrong["jobs"]["fast"]["steps"][0]["env"][name] = "success".into();
        assert!(validate_fast_gate(&wrong).is_err());
        let mut bypass = original.clone();
        bypass["jobs"]["fast"]["steps"][0]["run"] = script
            .replace(&format!("test \"${name}\" = success"), "true")
            .into();
        assert!(validate_fast_gate(&bypass).is_err());
    }
    for needs in [
        serde_json::json!(["native"]),
        serde_json::json!(["python-android"]),
        serde_json::json!([]),
    ] {
        let mut changed = original.clone();
        changed["jobs"]["fast"]["needs"] = needs;
        assert!(validate_fast_gate(&changed).is_err());
    }
    for (key, value) in [("if", "${{ success() }}"), ("name", "different check")] {
        let mut changed = original.clone();
        changed["jobs"]["fast"][key] = value.into();
        assert!(validate_fast_gate(&changed).is_err());
    }
}
