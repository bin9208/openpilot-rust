//! Frozen independent original feedback checker/daemon contracts, not candidate policy.
use openpilot_dev_tools::feedbackd_check::{
    input_packet, iteration_contract, validate_bookmark, validate_source,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/feedbackd")
        .join(name)
}
fn manifest() -> Value {
    let raw = fs::read(fixture("manifest.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "f8c68fc0d8abe7c98ed194b923fb301a2b6639e92e4629d5592c39fbaf974c1a"
    );
    serde_json::from_slice(&raw).unwrap()
}
fn original() -> Value {
    serde_json::from_slice(&fs::read(fixture("original.json")).unwrap()).unwrap()
}
fn unhex(raw: &str) -> Vec<u8> {
    raw.as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn immutable_original_live_packets_cli_and_failure_provenance_is_complete() {
    let manifest = manifest();
    assert_eq!(manifest["source_sha256"].as_object().unwrap().len(), 36);
    assert_eq!(
        manifest["counts"],
        serde_json::json!({"iterations":8,"bookmarks":4,"packets":6,"cli":8,"child_failures":3})
    );
    for (name, sha) in manifest["files"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(fixture(name)).unwrap())),
            sha.as_str().unwrap(),
            "{name}"
        );
    }
    let original = original();
    assert_eq!(original["rows"].as_array().unwrap().len(), 8);
    assert_eq!(original["bookmarks"].as_array().unwrap().len(), 4);
    assert_eq!(original["input_packets"].as_array().unwrap().len(), 6);
    assert_eq!(original["bookmark_log_count"], 4);
    assert_eq!(original["audio_recording_disabled"], true);
    assert_eq!(original["readiness_ms"], 8000);
    assert_eq!(original["subscriber_ms"], 200);
    assert_eq!(original["shutdown_ms"], 3000);
}
#[test]
fn packet_constructors_keep_full_original_raw_audio_and_car_button_payloads() {
    use openpilot_cereal::log_capnp::event;
    for source in original()["input_packets"].as_array().unwrap() {
        let topic = source["topic"].as_str().unwrap();
        let valid = source["valid"].as_bool().unwrap();
        let now = source["logMonoTime"].as_u64().unwrap();
        let raw = input_packet(topic, valid, now).unwrap();
        let message = capnp::serialize::read_message(
            &mut std::io::Cursor::new(raw),
            capnp::message::ReaderOptions::new(),
        )
        .unwrap();
        let event = message.get_root::<event::Reader>().unwrap();
        assert_eq!(event.get_valid(), valid);
        assert_eq!(event.get_log_mono_time(), now);
        match (topic, event.which().unwrap()) {
            ("rawAudioData", event::RawAudioData(payload)) => {
                let payload = payload.unwrap();
                assert_eq!(payload.get_sample_rate(), 16000);
                assert_eq!(
                    payload.get_data().unwrap(),
                    unhex(source["data_hex"].as_str().unwrap())
                );
            }
            ("carState", event::CarState(payload)) => {
                let payload = payload.unwrap();
                assert!(payload.get_can_valid());
                let buttons = payload.get_button_events().unwrap();
                assert_eq!(buttons.len(), 1);
                assert_eq!(
                    buttons.get(0).get_type().unwrap(),
                    openpilot_cereal::car_capnp::car_state::button_event::Type::Lkas
                );
                assert_eq!(buttons.get(0).get_pressed(), valid);
            }
            ("bookmarkButton", event::BookmarkButton(_)) => {}
            _ => panic!("wrong input topic {topic}"),
        }
    }
}
#[test]
fn all_original_bookmarks_preserve_union_validity_and_inclusive_time_boundaries() {
    let original = original();
    for source in original["bookmarks"].as_array().unwrap() {
        let raw = unhex(source["raw_hex"].as_str().unwrap());
        let now = source["logMonoTime"].as_u64().unwrap();
        validate_bookmark(&raw, now, now).unwrap();
        assert!(validate_bookmark(&raw, now + 1, now + 2).is_err());
        assert!(validate_bookmark(&raw, now - 2, now - 1).is_err());
    }
    assert!(
        original["invalid_bookmark"]["rejected_by_original_assertion"]
            .as_bool()
            .unwrap()
    );
    assert!(validate_bookmark(
        &unhex(original["invalid_bookmark"]["raw_hex"].as_str().unwrap()),
        0,
        u64::MAX
    )
    .is_err());
    let wrong = original["input_packets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["topic"] == "bookmarkButton" && v["valid"] == true)
        .unwrap();
    assert!(validate_bookmark(&unhex(wrong["raw_hex"].as_str().unwrap()), 0, u64::MAX).is_err());
    assert!(validate_bookmark(b"invalid capnp", 0, u64::MAX).is_err());
}
#[test]
fn all_eight_source_iterations_include_invalid_bookmark_forwarding_and_zero_audio() {
    for source in original()["iterations"].as_array().unwrap() {
        let i = source["index"].as_u64().unwrap() as usize;
        let actual = iteration_contract(i).unwrap();
        assert_eq!(actual.index, i);
        assert_eq!(
            actual.bookmarks,
            source["bookmarks"].as_u64().unwrap() as usize
        );
        assert_eq!(actual.car_valid, source["car_valid"].as_bool().unwrap());
        assert_eq!(actual.bookmark_valid, source["bookmark_valid"].as_bool());
        assert_eq!(actual.audio_feedback, 0);
        assert_eq!(actual.raw_audio_samples, 1536);
        assert_eq!(actual.sample_rate, 16000);
    }
    assert!(iteration_contract(8).is_err());
}
#[test]
fn only_the_checker_may_disappear_while_all_36_source_pins_reject_drift() {
    let pins = manifest()["source_sha256"].clone();
    assert_eq!(validate_source(&root()).unwrap(), pins);
    let copied = tempfile::tempdir().unwrap();
    for (name, _) in pins.as_object().unwrap() {
        let path = copied.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Ok(raw) = fs::read(root().join(name)) {
            fs::write(path, raw).unwrap();
        }
    }
    for (name, _) in pins.as_object().unwrap() {
        let path = copied.path().join(name);
        let before = fs::read(&path).ok();
        fs::write(&path, b"changed source\n").unwrap();
        assert!(validate_source(copied.path()).is_err(), "changed {name}");
        fs::remove_file(&path).unwrap();
        assert_eq!(
            validate_source(copied.path()).is_ok(),
            name == "rust/tools/check_feedbackd.py",
            "missing {name}"
        );
        if let Some(raw) = before {
            fs::write(path, raw).unwrap();
        }
    }
    let _ = fs::remove_file(copied.path().join("rust/tools/check_feedbackd.py"));
    assert_eq!(validate_source(copied.path()).unwrap(), pins);
    // A retired file is permitted to be absent, not a present dangling link.
    // read() alone reports NotFound for both, so exercise the actual filesystem.
    std::os::unix::fs::symlink(
        "missing-feedback-checker-target.py",
        copied.path().join("rust/tools/check_feedbackd.py"),
    )
    .unwrap();
    assert!(
        validate_source(copied.path()).is_err(),
        "present dangling retired checker"
    );
}
