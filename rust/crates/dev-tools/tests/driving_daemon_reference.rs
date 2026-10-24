use openpilot_dev_tools::driving_daemon_reference::{self as native, Original};
use serde_json::Value;
use std::{env, path::PathBuf};

fn observation() -> Original {
    let directory = PathBuf::from(
        env::var_os("DRIVING_ORIGINAL_OBSERVATION")
            .expect("cloud must provide the corrected original-only capture directory"),
    );
    let digest = env::var("DRIVING_ORIGINAL_MANIFEST_SHA256")
        .expect("cloud must pin the corrected original manifest SHA-256");
    Original::open(&directory, &digest)
        .expect("complete original capture must pass all file and schedule seals")
}

#[test]
fn complete_original_roster_and_packed_input_are_independent() {
    let original = observation();
    assert_eq!(original.scenarios().len(), 6);
    assert_eq!(
        original
            .scenarios()
            .iter()
            .map(|case| case["frames"].as_array().unwrap().len())
            .sum::<usize>(),
        40
    );
    assert_eq!(
        original
            .scenarios()
            .iter()
            .flat_map(|case| case["frames"].as_array().unwrap())
            .filter(|frame| !frame["publications"].as_object().unwrap().is_empty())
            .count(),
        38
    );
    original.verify().unwrap();
}

#[test]
fn original_cereal_packet_projects_to_original_decoded_fields_and_detects_mutation() {
    let original = observation();
    let case = &original.scenarios()[0];
    let row = &case["frames"][0];
    let source = &row["publications"]["modelV2"];
    let packet = original.read(source["packet"].as_str().unwrap()).unwrap();
    let expected: Value =
        serde_json::from_slice(&original.read(source["decoded"].as_str().unwrap()).unwrap())
            .unwrap();
    let raw = original
        .read(row["raw_prediction"].as_str().unwrap())
        .unwrap();
    let same = native::compare_packet(&expected, &packet, "modelV2", 0, 1, Some(&raw)).unwrap();
    assert!(same.fields > 100);
    assert_eq!(same.model_execution_time, 0.5);
    assert_eq!(same.log_mono_time, 0);
    assert!(native::compare_packet(&expected, &packet, "modelV2", 1, 2, Some(&raw)).is_err());
    let mut changed = expected.clone();
    changed["modelV2"]["frameId"] = 999_999.into();
    assert!(native::compare_packet(&changed, &packet, "modelV2", 0, 1, Some(&raw)).is_err());
    let mut changed_raw = raw;
    changed_raw[0] ^= 1;
    assert!(
        native::compare_packet(&expected, &packet, "modelV2", 0, 1, Some(&changed_raw)).is_err()
    );
}

#[test]
fn every_original_publication_packet_projects_to_its_independent_decoded_row() {
    let original = observation();
    let mut checked = 0;
    for case in original.scenarios() {
        for frame in case["frames"].as_array().unwrap() {
            for topic in ["modelV2", "drivingModelData", "cameraOdometry"] {
                let Some(source) = frame["publications"].get(topic) else {
                    continue;
                };
                let packet = original.read(source["packet"].as_str().unwrap()).unwrap();
                let expected: Value = serde_json::from_slice(
                    &original.read(source["decoded"].as_str().unwrap()).unwrap(),
                )
                .unwrap();
                let raw = if topic == "modelV2" && case["mode"] == "dual" {
                    Some(
                        original
                            .read(frame["raw_prediction"].as_str().unwrap())
                            .unwrap(),
                    )
                } else {
                    None
                };
                if topic == "modelV2" {
                    assert_eq!(raw.is_some(), case["mode"] == "dual");
                    assert_eq!(
                        expected["modelV2"]["rawPredictions"].is_object(),
                        case["mode"] == "dual"
                    );
                }
                let compared =
                    native::compare_packet(&expected, &packet, topic, 0, 1, raw.as_deref())
                        .unwrap();
                assert!(compared.fields > 0);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 114);
}
