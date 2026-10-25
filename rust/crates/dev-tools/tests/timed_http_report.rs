//! Original HTTP report dictionaries retain insertion order below the root.
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    timed_check::{run, Kind, Options},
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
struct NeverCandidate;
impl Runner for NeverCandidate {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        Err("source-only worker executed candidate".into())
    }
}
#[test]
fn original_http_report_nested_request_key_order() {
    let bytes = include_bytes!("fixtures/timed-tools/http-report-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "51a2e0230cfcd7a28ff2f5cf95ce5f4ab1a7de706186f81e08a1537c6de3309d"
    );
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let options = Options {
        root: Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../..")).into(),
        binary: "/unused-candidate".into(),
        output: directory.path().join("artifacts"),
        reference: env!("CARGO_BIN_EXE_timed-source-reference").into(),
        command: env!("CARGO_BIN_EXE_timed-command-fixture").into(),
        worker: vec!["success".into(), "python".into()],
    };
    run(Kind::Http, &options, &mut NeverCandidate).unwrap();
    let report =
        JsonValue::parse(&fs::read_to_string(options.output.join("result.json")).unwrap()).unwrap();
    let JsonView::Object(fields) = report.view() else {
        panic!("report not object")
    };
    let content = fields
        .into_iter()
        .map(|(key, value)| {
            let key = JsonValue::codepoints(key.to_vec()).unwrap();
            format!(
                "{}:{}",
                key.to_json().unwrap(),
                if key.text_eq("elapsed") {
                    "0.0".to_owned()
                } else {
                    value.to_json().unwrap()
                }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let actual = JsonValue::parse(&format!("{{{content}}}"))
        .unwrap()
        .to_json()
        .unwrap();
    assert_eq!(
        actual,
        JsonValue::parse(original["report_without_elapsed"].as_str().unwrap())
            .unwrap()
            .to_json()
            .unwrap()
    );
}
