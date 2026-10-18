//! Original arithmetic admission and log-before-error/command boundaries.
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    timed_fixtures::Fixture,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

#[test]
fn original_signed_zero_nonfinite_and_arbitrary_integer_action_boundaries() {
    let bytes = include_bytes!("fixtures/timed-tools/actions-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "6d3d76552b99654efa284a8e8b6bfd72e4fb2c26246b84bb30b705f09c30194e"
    );
    let data: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 15);
    let mut expected = vec![];
    let mut actual = vec![];
    for row in data["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let fixture = Fixture::new(
            temporary.path(),
            "UTC",
            Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
        )
        .unwrap();
        let mut config = fixture.config.clone();
        config.as_object_mut().unwrap().remove("actions");
        let mut text = serde_json::to_string(&config).unwrap();
        assert_eq!(text.pop(), Some('}'));
        text.push_str(&format!(
            ",\"actions\":[{}]}}",
            row["action_json"].as_str().unwrap()
        ));
        // Keep arbitrary JSON integers/nonfinite literals intact. Serde is
        // deliberately not used to round-trip an input arithmetic operand.
        let input = format!("{{\"config\":{text},\"loop\":false}}\n");
        let mut spec = CommandSpec::new_os(
            vec![env!("CARGO_BIN_EXE_timed-source-reference").into()],
            Duration::from_secs(30),
        );
        fixture.configure(&mut spec);
        spec.input = Some(input.into_bytes());
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let result = NativeRunner.run(&spec).unwrap();
        assert_eq!(
            result.code,
            0,
            "{}: {}",
            row["action_json"],
            String::from_utf8_lossy(&result.stderr)
        );
        actual.push(serde_json::from_slice::<Value>(&result.stdout).unwrap());
        expected.push(row["expected"].clone());
    }
    assert_eq!(actual, expected);
}
