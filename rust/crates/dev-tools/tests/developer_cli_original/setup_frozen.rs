//! setup21의 역사 JSON만 수용한다. 원본 실행·현재 expected 생성·retirement 판정은 없다.
use super::{Case, Value};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(super) const RAW: &[u8] =
    include_bytes!("../fixtures/setup-ssh-original/setup-effects-original.json");
const SHA256: &str = "444a47ecba9579d4042f26dd89f8438908b56d244a0f2c98c8b0dd3e1ef3a6ef";
const BYTES: usize = 13_800;
const GROUP: &str = "setup-effects";
type Admission<T> = std::result::Result<T, &'static str>;

fn require(condition: bool, error: &'static str) -> Admission<()> {
    if condition {
        Ok(())
    } else {
        Err(error)
    }
}

fn keys(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == names.len() && names.iter().all(|name| object.contains_key(*name))
    })
}

fn validate(group: &str, envelope: &Value, cases: &[Case]) -> Admission<()> {
    require(group == GROUP, "setup frozen scope differs")?;
    require(
        keys(envelope, &["schema", "group", "metadata", "records"]),
        "setup envelope object differs",
    )?;
    require(
        envelope["schema"] == "openpilot.developer-cli-original.v1" && envelope["group"] == GROUP,
        "setup envelope scope differs",
    )?;
    // native_sources는 당시 미기록이다. 추가 필드/현재 provider 대입도 equality로 거부한다.
    require(
        envelope["metadata"]
            == json!({
                "python":"3.12.15 (main, Oct  1 2026, 02:35:59) [GCC 13.3.0]",
                "requests":"2.34.2",
                "sources":{
                    "tools/scripts/setup_ssh_keys.py":"ab157e4943734c969ffc1e8113dc9d09ded355f3463da34c2b9132fc24181f3f",
                    "tools/scripts/ssh.py":"f4fb8dff60a0bab57643172093a6f437acb0d001929103f081d872280710eeff"
                }
            }),
        "setup historical metadata differs",
    )?;
    let records = envelope["records"]
        .as_array()
        .ok_or("setup records array required")?;
    require(records.len() == 21, "setup record count differs")?;
    let mut ids = BTreeSet::new();
    for record in records {
        require(
            keys(record, &["input", "criteria", "expected_original"]),
            "setup record object differs",
        )?;
        let input = &record["input"];
        require(input["tool"] == "setup", "setup record tool differs")?;
        let id = input["id"].as_str().ok_or("setup string id required")?;
        require(!id.is_empty() && ids.insert(id), "setup ids must be unique")?;
        require(
            record["criteria"] == json!({"kind":"exact"}),
            "setup exact criteria required",
        )?;
        let output = &record["expected_original"];
        require(
            keys(
                output,
                &[
                    "stdout",
                    "stderr",
                    "exit_code",
                    "exception",
                    "effects",
                    "writes",
                    "exec",
                ],
            ) && output["stdout"].is_string()
                && output["stderr"].is_string()
                && (output["exit_code"].is_null() || output["exit_code"].is_i64())
                && output["effects"].is_array()
                && output["writes"].is_array()
                && output["exec"].is_null(),
            "setup original output type differs",
        )?;
        let exception = &output["exception"];
        require(
            exception.is_null()
                || (keys(exception, &["type", "message"])
                    && exception["type"].is_string()
                    && exception["message"].is_string()),
            "setup original exception type differs",
        )?;
    }
    require(cases.len() == 21, "setup authored case count differs")?;
    for (case, record) in cases.iter().zip(records) {
        require(
            case.input == record["input"],
            "setup authored input differs",
        )?;
        require(
            case.criteria == record["criteria"],
            "setup authored criteria differs",
        )?;
    }
    Ok(())
}

fn admit_bytes(raw: &[u8], pin: &str, group: &str, cases: &[Case]) -> Admission<Value> {
    // pin은 설정값이 아니다. 거부 control도 임의 pin으로 새 expected를 수용할 수 없다.
    require(pin == SHA256, "setup fixed pin differs")?;
    require(raw.len() == BYTES, "setup raw length differs")?;
    require(
        format!("{:x}", Sha256::digest(raw)) == SHA256,
        "setup raw SHA256 differs",
    )?;
    let envelope: Value = serde_json::from_slice(raw).map_err(|_| "setup raw JSON invalid")?;
    validate(group, &envelope, cases)?;
    Ok(envelope)
}

pub(super) fn admit(group: &str, cases: &[Case]) -> Admission<Value> {
    admit_bytes(RAW, SHA256, group, cases)
}

// 아래 fixture-derived 입력은 거부 controls 전용이다. 실제 semantic 비교의 authored21은
// 부모 파일의 기존 test body가 계속 독립 구성하며 candidate/verify_envelope를 바꾸지 않는다.
fn rejection_inputs() -> (Value, Vec<Case>) {
    let envelope: Value = serde_json::from_slice(RAW).unwrap();
    let cases = envelope["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| Case {
            input: record["input"].clone(),
            criteria: record["criteria"].clone(),
        })
        .collect();
    (envelope, cases)
}

#[test]
fn setup_frozen_rejects_wrong_pin_last_byte_truncation_and_extra_bytes() {
    assert_eq!(
        admit_bytes(RAW, &"0".repeat(64), GROUP, &[]).unwrap_err(),
        "setup fixed pin differs"
    );
    let mut changed = RAW.to_vec();
    *changed.last_mut().unwrap() ^= 1;
    assert_eq!(
        admit_bytes(&changed, SHA256, GROUP, &[]).unwrap_err(),
        "setup raw SHA256 differs"
    );
    assert_eq!(
        admit_bytes(&RAW[..RAW.len() - 1], SHA256, GROUP, &[]).unwrap_err(),
        "setup raw length differs"
    );
    let mut extra = RAW.to_vec();
    extra.push(b'\n');
    assert_eq!(
        admit_bytes(&extra, SHA256, GROUP, &[]).unwrap_err(),
        "setup raw length differs"
    );
}

#[test]
fn setup_frozen_rejects_wrong_scope_metadata_roster_and_types() {
    let (envelope, cases) = rejection_inputs();
    assert_eq!(
        admit("ssh-plan", &cases).unwrap_err(),
        "setup frozen scope differs"
    );
    for (pointer, value, error) in [
        (
            "/group",
            json!("ssh-parser"),
            "setup envelope scope differs",
        ),
        (
            "/metadata/requests",
            json!("current"),
            "setup historical metadata differs",
        ),
        (
            "/records/0/input/tool",
            json!("ssh"),
            "setup record tool differs",
        ),
        ("/records/0/input/id", json!(1), "setup string id required"),
        (
            "/records/1/input/id",
            json!("missing-username"),
            "setup ids must be unique",
        ),
        (
            "/records/0/criteria/kind",
            json!("exception-pair"),
            "setup exact criteria required",
        ),
        (
            "/records/0/expected_original/stdout",
            json!([]),
            "setup original output type differs",
        ),
    ] {
        let mut bad = envelope.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(validate(GROUP, &bad, &cases).unwrap_err(), error);
    }
    let mut bad = envelope.clone();
    bad["metadata"]["native_sources"] = json!({});
    assert_eq!(
        validate(GROUP, &bad, &cases).unwrap_err(),
        "setup historical metadata differs"
    );
    let mut bad = envelope;
    let extra = bad["records"][0].clone();
    bad["records"].as_array_mut().unwrap().push(extra);
    assert_eq!(
        validate(GROUP, &bad, &cases).unwrap_err(),
        "setup record count differs"
    );
}

#[test]
fn setup_frozen_rejects_authored_input_and_criteria_drift() {
    let (envelope, mut cases) = rejection_inputs();
    cases[0].input["args"] = json!(["changed"]);
    assert_eq!(
        validate(GROUP, &envelope, &cases).unwrap_err(),
        "setup authored input differs"
    );
    cases[0].input = envelope["records"][0]["input"].clone();
    cases[0].criteria = json!({"kind":"exact", "extra":true});
    assert_eq!(
        validate(GROUP, &envelope, &cases).unwrap_err(),
        "setup authored criteria differs"
    );
}
