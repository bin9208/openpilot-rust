//! Native counterparts of tools/ci/tests/test_workflow_policy.py.
use openpilot_dev_tools::ci_policy::{check_workflow_policy, parse};
use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn inputs() -> Vec<(String, String)> {
    [
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
fn maintained_workflow_policy_is_valid() {
    let files = inputs();
    check_workflow_policy(&files).unwrap();
}
#[test]
fn protected_workflow_mutations_are_rejected() {
    let original = inputs();
    for (name, from, to) in [
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
fn diagnostic_common_helpers_match_verified_7766383_bytes_without_reexecution() {
    use sha2::{Digest, Sha256};
    let contract: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root().join("tools/ci/diagnostic-296-contract.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        contract["common_source"],
        "77663836a95a9e2aff9aa66592d8b06ed9c87cd9"
    );
    assert_eq!(contract["common_files"].as_object().unwrap().len(), 4);
    for (name, seal) in contract["common_files"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root().join(name)).unwrap())),
            seal["sha256"].as_str().unwrap(),
            "common source drift {name}"
        );
    }
}
