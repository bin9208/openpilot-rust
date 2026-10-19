//! Independent original and reviewed #392 playback expectations, never candidate output.
#![cfg(feature = "soundd-policy-native")]

#[path = "support/soundd_policy_json.rs"]
mod soundd_policy_json;

use flate2::read::GzDecoder;
use openpilot_dev_tools::soundd_policy_reference as reference;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

fn repository() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}

fn corpus() -> Value {
    let bytes = include_bytes!("fixtures/soundd-policy/original.json.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "82ef895663d96cfde1f33da0e22c4785a007c05a2dcab43db0b9e76a2c4c54b1"
    );
    let mut raw = Vec::new();
    GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut raw)
        .unwrap();
    soundd_policy_json::parse(&raw)
}

fn assert_rows(actual: &[Value], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    for (step, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(a.as_object().unwrap().len(), 2, "step {step}");
        assert_eq!(
            a["state"].as_object().unwrap().len(),
            b["state"].as_object().unwrap().len()
        );
        for key in ["alert", "frame", "adjust", "countdown", "timeout"] {
            assert_eq!(a["state"][key], b["state"][key], "step {step} {key}");
        }
        for key in ["volume", "filter"] {
            let x = a["state"][key].as_f64().unwrap();
            let y = b["state"][key].as_f64().unwrap();
            assert!(
                (x - y).abs() <= 1e-12_f64.max(1e-12 * x.abs().max(y.abs())),
                "step {step} {key}: {x} != {y}"
            );
        }
        let bits = |row: &Value| {
            row["samples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v.as_f64().unwrap() as f32).to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(a), bits(b), "step {step}: sample bits");
    }
}

#[test]
fn all_141_requests_are_generated_without_captured_request_lookup() {
    let original = corpus();
    assert_eq!(
        original["cases"][0]["config"]["steps"][11]["input"]["now"]
            .as_f64()
            .unwrap()
            .to_bits(),
        (11_f64 * 1.1).to_bits()
    );
    let actual = reference::steps();
    assert_eq!(actual.len(), 141);
    for row in original["cases"].as_array().unwrap() {
        for (index, (a, b)) in actual
            .iter()
            .zip(row["config"]["steps"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(a, b, "request {index}");
        }
    }
}

#[test]
fn all_six_legacy_and_corrected_source_schedules_match_independent_capture() {
    let original = corpus();
    for row in original["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = row["config"].clone();
        let rate = row["rate"].as_u64().unwrap() as u32;
        let assets = if rate == 0 {
            repository().join("openpilot/selfdrive/assets")
        } else {
            reference::create_assets(
                directory.path(),
                rate,
                row["channels"].as_u64().unwrap() as u16,
            )
            .unwrap()
        };
        config["assets"] = json!(assets);
        if config.get("basedir").is_some() {
            config["basedir"] = json!(repository());
        }
        let legacy = reference::reference(repository(), &config, false).unwrap();
        let corrected = reference::reference(repository(), &config, true).unwrap();
        assert_rows(&legacy, &row["original"]);
        assert_rows(&corrected, &row["intended"]);
        for (a, b) in legacy.iter().zip(&corrected) {
            for key in [
                "alert",
                "volume",
                "adjust",
                "countdown",
                "timeout",
                "filter",
            ] {
                assert_eq!(a["state"][key], b["state"][key], "non-frame {key}");
            }
        }
    }
}

#[test]
fn reference_reads_actual_wave_bytes_and_variable_steps() {
    let directory = tempfile::tempdir().unwrap();
    let assets = reference::create_assets(directory.path(), 48000, 1).unwrap();
    let path = assets.join("sounds_eng/engage.wav");
    let mut bytes = fs::read(&path).unwrap();
    let pcm: Vec<u8> = [0_i16, 8192, -8192, 16384]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    let end = bytes.len();
    bytes[end - pcm.len()..].copy_from_slice(&pcm);
    fs::write(&path, bytes).unwrap();
    let config = json!({
        "assets": assets, "engage_volume": 1.0, "language": "en", "tizi": false,
        "steps": [{"input": {"now": 0.0, "updated_selfdrive": true,
            "updated_carrot": false, "selfdrive_received": 0.0, "enabled": false,
            "alert": 1, "countdown": -1, "pressure": null, "main_buttons": []},
            "frames": 6, "adjust": 1.0, "language": null}]
    });
    let actual = reference::reference(repository(), &config, true).unwrap();
    assert_eq!(actual.len(), 1);
    let expected: Vec<_> = [0_f32, 0.025, -0.025, 0.05, 0., 0.]
        .into_iter()
        .map(f32::to_bits)
        .collect();
    let observed: Vec<_> = actual[0]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v: &Value| (v.as_f64().unwrap() as f32).to_bits())
        .collect();
    assert_eq!(observed, expected);
    assert_eq!(actual[0]["state"]["frame"], 4);
}

#[test]
fn original_policy_inputs_are_pinned_before_reference_use() {
    let original = corpus();
    let mut actual = reference::sources(repository()).unwrap();
    actual.extend(reference::historical_sources());
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        original["provenance"]["source_sha256"]
    );
}
