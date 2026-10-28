// All 768 transitions retain full-state/result bits and atomic error checks.
#[path = "support/bosch_tracking_state.rs"]
mod support;

#[test]
fn raw_track_identity_coasting_diagnostics_and_atomic_errors_match_source() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = temp.path().join("original-inputs.json");
    let reference = temp.path().join("original-results.json");
    let evidence = std::env::var_os("RUNNER_TEMP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned())
        .join("bosch-tracking-environment-607.json");
    std::fs::write(
        &inputs,
        serde_json::to_vec(&support::frozen_fixture()).unwrap(),
    )
    .unwrap();
    let oracle = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/bosch_tracking_reference.py");
    let status = std::process::Command::new("python3")
        .arg(oracle)
        .arg(inputs)
        .arg(&reference)
        .arg(evidence)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "hash-verified original tracking oracle failed"
    );
    support::check(
        &serde_json::from_slice::<Vec<serde_json::Value>>(&std::fs::read(reference).unwrap())
            .unwrap(),
    );
}
