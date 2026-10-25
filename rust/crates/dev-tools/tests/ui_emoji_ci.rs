#[test]
fn ui_connectivity_builds_tests_and_runs_native_emoji_gate() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = std::fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap();
    let workflow = openpilot_dev_tools::ci_policy::parse(&workflow).unwrap();
    let job = workflow["jobs"]["ui-connectivity"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(job.contains("--bin check-ui-emoji"));
    assert!(job.contains("--test ui_emoji_check"));
    assert!(job.contains("rust/target/debug/check-ui-emoji --binary"));
    assert!(!job.contains("python rust/tools/check_ui_emoji.py"));
}
