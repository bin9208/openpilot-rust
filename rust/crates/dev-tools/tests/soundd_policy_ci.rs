//! Mandatory native policy/helper route with the retained seven-scenario daemon gate.
const WORKFLOW: &str = include_str!("../../../../.github/workflows/rust.yml");
#[test]
fn sensor_audio_builds_and_tests_native_policy_and_assets_before_comparisons() {
    // Inspect the entire sensor-audio region (the following job starts at two spaces).
    let region = WORKFLOW.split("\n  sensor-audio:\n").nth(1).unwrap();
    let end = region
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            Some((start, line))
        })
        .find(|(_, line)| {
            line.starts_with("  ") && !line.starts_with("   ") && !line.trim().is_empty()
        })
        .map_or(region.len(), |(start, _)| start);
    let region = &region[..end];
    for needle in ["--features soundd-policy-native --bin check-soundd-policy --bin create-soundd-fixture-assets","--test soundd_policy_reference --test soundd_policy_check --test soundd_fixture_assets --test soundd_policy_retirement","cargo clippy","-- -D warnings","rustup component add clippy","debug/check-soundd-policy\" --binary","--fixture-assets-binary \"$PWD/rust/target/debug/create-soundd-fixture-assets\"","check_soundd_daemon.py","build_msgq_python.py","build_params_python.py","miri test"] { assert!(region.contains(needle),"missing soundd route: {needle}"); }
    assert!(!region.contains("python rust/tools/check_soundd.py"));
    assert!(
        region
            .find("--features soundd-policy-native --bin check-soundd-policy")
            .unwrap()
            < region.find("debug/check-soundd-policy\" --binary").unwrap()
    );
}
