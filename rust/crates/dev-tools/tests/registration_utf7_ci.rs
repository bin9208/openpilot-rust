#[test]
fn startup_builds_and_runs_native_utf7_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = std::fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap();
    let job = workflow
        .split("  startup-runtime:")
        .nth(1)
        .unwrap()
        .split("\n  startup-services:")
        .next()
        .unwrap();
    assert!(job.contains(
        "rust/target/debug/check-registration-utf7 rust/target/debug/examples/registration_utf7"
    ));
    assert!(job.contains("--test registration_utf7_check --test registration_utf7_ci"));
    assert!(!job.contains("python rust/tools/check_registration_utf7.py"));
}
