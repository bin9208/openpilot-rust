//! Mandatory native policy/helper route with the retained seven-scenario daemon gate.
const WORKFLOW: &str = include_str!("../../../../.github/workflows/rust.yml");
#[test]
fn sensor_audio_builds_and_tests_native_policy_and_assets_before_comparisons() {
    let workflow = openpilot_dev_tools::ci_policy::parse(WORKFLOW).unwrap();
    let region = workflow["jobs"]["sensor-audio"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for needle in ["--features soundd-policy-native --bin check-soundd-policy --bin create-soundd-fixture-assets","--test soundd_policy_reference --test soundd_policy_check --test soundd_fixture_assets --test soundd_policy_retirement","cargo clippy","-- -D warnings","rustup component add clippy","debug/check-soundd-policy\" --binary","--fixture-assets-binary \"$SOUNDD_FIXTURE_ASSETS_BINARY\"","test ! -e rust/tools/check_soundd_daemon.py","build_msgq_python.py","build_params_python.py","miri test"] { assert!(region.contains(needle),"missing soundd route: {needle}"); }
    assert!(!region.contains("python rust/tools/check_soundd.py"));
    assert!(
        region
            .find("--features soundd-policy-native --bin check-soundd-policy")
            .unwrap()
            < region.find("debug/check-soundd-policy\" --binary").unwrap()
    );
}
