// Every result, debug value and full filter state is mandatory in the ordinary
// regression suite. The oracle is the pinned original in this same environment.
#[path = "support/bosch_full_state.rs"]
mod support;
use serde_json::Value;
use support::check;

#[test]
fn same_environment_original_source_full_filter_states_match_bits() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = temp.path().join("original-inputs.jsonl");
    let reference = temp.path().join("original-results.jsonl");
    let evidence = std::env::var_os("RUNNER_TEMP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned())
        .join("bosch-source-environment-607.json");
    let mut decoder = support::frozen_fixture();
    std::io::copy(&mut decoder, &mut std::fs::File::create(&inputs).unwrap()).unwrap();
    let oracle = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/bosch_acceleration_reference.py");
    let status = std::process::Command::new("python3")
        .arg(oracle)
        .arg(inputs)
        .arg(&reference)
        .arg(evidence)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "hash-verified original source oracle failed"
    );
    check(
        serde_json::Deserializer::from_reader(std::io::BufReader::new(
            std::fs::File::open(reference).unwrap(),
        ))
        .into_iter::<Value>()
        .map(Result::unwrap),
    );
}
