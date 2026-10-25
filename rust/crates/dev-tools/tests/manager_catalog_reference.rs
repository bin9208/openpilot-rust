//! Source-only original constructor, callback/Params order and request admission.
use openpilot_dev_tools::manager_catalog_reference::{evaluate, Context};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/manager-catalog")
        .join(name)
}
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn equal(a: JsonValue, b: JsonValue) {
    let mut pending = vec![(a, b)];
    while let Some((a, b)) = pending.pop() {
        match (a.view(), b.view()) {
            (JsonView::Null, JsonView::Null) => {}
            (JsonView::Bool(a), JsonView::Bool(b)) => assert_eq!(a, b),
            (JsonView::Integer(a), JsonView::Integer(b)) => assert_eq!(a, b),
            (JsonView::Float(a), JsonView::Float(b)) => assert_eq!(a.to_bits(), b.to_bits()),
            (JsonView::Text(a), JsonView::Text(b)) => assert_eq!(a, b),
            (JsonView::Array(a), JsonView::Array(b)) => {
                assert_eq!(a.len(), b.len());
                pending.extend(a.into_iter().zip(b));
            }
            (JsonView::Object(a), JsonView::Object(b)) => {
                assert_eq!(a.len(), b.len());
                for (key, value) in a {
                    let (_, other) = b.iter().find(|(name, _)| *name == key).unwrap();
                    pending.push((value, other.clone()));
                }
            }
            _ => panic!(
                "source JSON types differ: {} != {}",
                a.to_json().unwrap(),
                b.to_json().unwrap()
            ),
        }
    }
}
fn context(row: &Value) -> Context {
    let get = |name: &str| {
        row["environment_hex"][name]
            .as_str()
            .map(|s| OsString::from_vec(unhex(s)))
    };
    Context {
        tici: false,
        darwin: false,
        webcam: get("USE_WEBCAM"),
        carrot_web_external: get("CARROT_WEB_EXTERNAL"),
    }
}
#[test]
fn all_116_original_scenarios_preserve_full_descriptors_callbacks_and_fatal_access_prefix() {
    let capture: Value =
        serde_json::from_slice(&fs::read(fixture("full-original.json")).unwrap()).unwrap();
    let rows = capture.as_array().unwrap();
    assert_eq!(rows.len(), 116);
    let temp = tempfile::tempdir().unwrap();
    let mut fatal = 0;
    for row in rows {
        let output = temp.path().join(row["name"].as_str().unwrap());
        let request = serde_json::to_vec(&row["request"]).unwrap();
        let result = evaluate(&request, &output, &context(row));
        if row["fatal"] == true {
            fatal += 1;
            let error = match result {
                Err(error) => error.to_string(),
                Ok(value) => panic!("{} must fail: {}", row["name"], value.to_json().unwrap()),
            };
            assert_eq!(error, "fatal_integer", "{}", row["name"]);
            let trace = JsonValue::parse(&fs::read_to_string(output.join("trace-0.json")).unwrap())
                .unwrap();
            equal(
                trace,
                JsonValue::parse(&row["expected"].to_string()).unwrap(),
            );
        } else {
            equal(
                result.unwrap_or_else(|e| panic!("{}: {e}", row["name"])),
                JsonValue::parse(&row["expected"].to_string()).unwrap(),
            );
        }
    }
    assert_eq!(fatal, 24);
}
#[test]
fn original_generic_json_and_dynamic_params_errors_are_not_normalized_or_hidden() {
    let capture: Value =
        serde_json::from_slice(&fs::read(fixture("generic-original.json")).unwrap()).unwrap();
    let rows = capture.as_array().unwrap();
    assert_eq!(rows.len(), 38);
    let temp = tempfile::tempdir().unwrap();
    for row in rows {
        let output = temp.path().join(row["name"].as_str().unwrap());
        let result = evaluate(
            &unhex(row["request_hex"].as_str().unwrap()),
            &output,
            &context(&Value::Null),
        );
        assert_eq!(
            result.is_ok(),
            row["source_exit"] == 0,
            "{} admission",
            row["name"]
        );
        assert_eq!(
            output.exists(),
            row["output_exists"].as_bool().unwrap(),
            "{} output phase",
            row["name"]
        );
        if let Some(expected) = row["result_text"].as_str() {
            equal(result.unwrap(), JsonValue::parse(expected).unwrap());
        } else {
            let error = match result {
                Err(error) => error.to_string(),
                Ok(value) => panic!("{} must fail: {}", row["name"], value.to_json().unwrap()),
            };
            if row["source_exit"] == -6 {
                assert_eq!(error, "fatal_integer");
            } else {
                assert_eq!(
                    error.split(':').next().unwrap(),
                    row["source_error_kind"].as_str().unwrap(),
                    "{} original request error category",
                    row["name"]
                );
            }
        }
        let actual_traces = if output.exists() {
            fs::read_dir(&output)
                .unwrap()
                .filter_map(|e| {
                    let e = e.unwrap();
                    let name = e.file_name().to_str().unwrap().to_owned();
                    (name.starts_with("trace-") && name.ends_with(".json")).then(|| {
                        (
                            name,
                            serde_json::from_slice::<Value>(&fs::read(e.path()).unwrap()).unwrap(),
                        )
                    })
                })
                .collect::<serde_json::Map<String, Value>>()
        } else {
            serde_json::Map::new()
        };
        assert_eq!(
            Value::Object(actual_traces),
            row["traces"],
            "{} exact ordered access files",
            row["name"]
        );
    }
}
#[test]
fn original_module_globals_preserve_getattr_call_and_deferred_serialization_phases() {
    use sha2::{Digest, Sha256};
    let raw = fs::read(fixture("globals-manifest.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "55bb03c863e162a8a68c5af6ff43abec1cc36c2af8b2f91ce35d7a519ac2c044"
    );
    let manifest: Value = serde_json::from_slice(&raw).unwrap();
    let raw = fs::read(fixture("globals-original.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        manifest["corpus_sha256"].as_str().unwrap()
    );
    // The final original predicate is a lone surrogate. Keep the fixture's
    // codepoints; serde_json::Value rejects that valid Python string.
    let capture = JsonValue::parse(std::str::from_utf8(&raw).unwrap()).unwrap();
    let JsonView::Array(rows) = capture.view() else {
        panic!("global capture array")
    };
    let text = |value: JsonValue| match value.view() {
        JsonView::Text(points) => points
            .iter()
            .map(|&p| char::from_u32(p).unwrap())
            .collect::<String>(),
        _ => panic!("capture text"),
    };
    assert_eq!(rows.len(), 20);
    let temp = tempfile::tempdir().unwrap();
    for (i, raw_row) in rows.iter().enumerate() {
        let mut fields = serde_json::Map::new();
        for key in ["request_hex", "error_kind", "traces"] {
            fields.insert(
                key.into(),
                serde_json::from_str(&raw_row.get(key).unwrap().to_json().unwrap()).unwrap(),
            );
        }
        fields.insert(
            "name".into(),
            Value::String(raw_row.get("name").unwrap().to_json().unwrap()),
        );
        fields.insert(
            "result_text".into(),
            match raw_row.get("result_text").unwrap().view() {
                JsonView::Null => Value::Null,
                _ => Value::String(text(raw_row.get("result_text").unwrap())),
            },
        );
        let row = Value::Object(fields);
        let output = temp.path().join(i.to_string());
        let result = evaluate(
            &unhex(row["request_hex"].as_str().unwrap()),
            &output,
            &context(&Value::Null),
        );
        if let Some(text) = row["result_text"].as_str() {
            equal(
                result.unwrap_or_else(|e| panic!("{}: {e}", row["name"])),
                JsonValue::parse(text).unwrap(),
            );
        } else {
            let error = match result {
                Err(error) => error.to_string(),
                Ok(_) => panic!("{} must fail", row["name"]),
            };
            assert_eq!(
                error.split(':').next().unwrap(),
                row["error_kind"].as_str().unwrap(),
                "{}",
                row["name"]
            );
            assert!(!output.join("result.json").exists());
        }
        let traces = fs::read_dir(output)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                let name = entry.file_name().into_string().unwrap();
                (name.starts_with("trace-") && name.ends_with(".json")).then(|| {
                    (
                        name,
                        serde_json::from_slice::<Value>(&fs::read(entry.path()).unwrap()).unwrap(),
                    )
                })
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(
            Value::Object(traces),
            row["traces"],
            "{} exact access order before serialization failure",
            row["name"]
        );
    }
}
#[test]
fn original_surrogate_process_paths_nul_paths_and_bytes_overflow_keep_error_phase() {
    use sha2::{Digest, Sha256};
    let raw = fs::read(fixture("generic-edges-original.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "8700e25c31a1b4df472e6a6361b2e24b3a3f0b04aee737c7ec73b85130b36045"
    );
    let rows: Vec<Value> = serde_json::from_slice(&raw).unwrap();
    assert_eq!(rows.len(), 5);
    let temp = tempfile::tempdir().unwrap();
    for row in rows {
        let output = temp.path().join(row["name"].as_str().unwrap());
        let result = evaluate(
            &unhex(row["request_hex"].as_str().unwrap()),
            &output,
            &context(&Value::Null),
        );
        let error = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("{} must fail", row["name"]),
        };
        let original = row["source"]["error"][0].as_str().unwrap();
        assert_eq!(
            error.split(':').next(),
            original.split(':').next(),
            "{} exact original failure class",
            row["name"]
        );
        assert_eq!(
            output.exists(),
            row["source"]["output_exists"].as_bool().unwrap()
        );
        assert!(!output.join("result.json").exists());
        assert!(!output.join("trace-0.json").exists());
    }
}
