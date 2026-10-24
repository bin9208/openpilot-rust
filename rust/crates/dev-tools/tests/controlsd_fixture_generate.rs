#![cfg(feature = "controlsd-fixture-native")]
use openpilot_dev_tools::controlsd_fixture_generate as fixture;
use openpilot_log_format::JsonValue;
use std::{fs, path::PathBuf, process::Command};

fn request(text: &str) -> JsonValue {
    fixture::generate(&JsonValue::parse(text).unwrap()).unwrap()
}
fn bytes(value: &JsonValue) -> Vec<u8> {
    serde_json::from_str(&value.to_json().unwrap()).unwrap()
}

#[test]
fn frame_preserves_actual_service_order_and_pose_builder_bytes() {
    let row = request(r#"{"op":"frame","index":3}"#);
    let packets: Vec<Vec<u8>> =
        serde_json::from_str(&row.get("messages").unwrap().to_json().unwrap()).unwrap();
    assert_eq!(packets.len(), 16);
    let services: Vec<_> = packets
        .iter()
        .map(|p| fixture::service(p).unwrap())
        .collect();
    assert_eq!(services, fixture::SERVICES);
    let time = 100.0 + 3.0 * 0.01;
    assert_eq!(
        packets[13],
        openpilot_dev_tools::paramsd_fixture_generate::pose(time, Default::default())
    );
    let projected = fixture::project_event(&packets[5]).unwrap();
    let model = projected.get("modelV2").unwrap();
    let x: Vec<f64> = serde_json::from_str(
        &model
            .get("orientation")
            .unwrap()
            .get("x")
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(x.len(), 33);
    assert!(x
        .iter()
        .all(|v| v.to_bits() == f64::from(0.01_f32).to_bits()));
}

#[test]
fn change_shallowly_replaces_nested_objects_and_keeps_other_packets_exact() {
    let row = request(r#"{"op":"frame","index":12}"#);
    let input = format!(
        r#"{{"op":"change","row":{},"updates":{{"carState":{{"cruiseState":{{"standstill":true}}}},"modelV2":{{"orientation":{{"y":[0.5]}}}}}},"omit":["carrotMan"],"settings":{{"DisableDM":1}}}}"#,
        row.to_json().unwrap()
    );
    let changed = request(&input);
    let before: Vec<Vec<u8>> =
        serde_json::from_str(&row.get("messages").unwrap().to_json().unwrap()).unwrap();
    let after: Vec<Vec<u8>> =
        serde_json::from_str(&changed.get("messages").unwrap().to_json().unwrap()).unwrap();
    assert_eq!(after.len(), 15);
    assert_eq!(before[2], after[2]);
    let car = fixture::project_event(&after[1])
        .unwrap()
        .get("carState")
        .unwrap();
    let cruise = car.get("cruiseState").unwrap();
    assert_eq!(cruise.get("enabled").unwrap().to_json().unwrap(), "false");
    assert_eq!(cruise.get("standstill").unwrap().to_json().unwrap(), "true");
    let model = fixture::project_event(&after[5])
        .unwrap()
        .get("modelV2")
        .unwrap();
    assert!(model.get("orientation").unwrap().get("x").is_none());
    assert_eq!(
        changed
            .get("params")
            .unwrap()
            .get("DisableDM")
            .unwrap()
            .to_json()
            .unwrap(),
        "[49]"
    );
}

#[test]
fn event_is_full_schema_but_service_api_is_bounded() {
    let packet = bytes(&request(
        r#"{"op":"event","service":"modelV2","time":1.25,"data":{"orientation":{"x":[0.125,-0.25]},"meta":{"desireState":[0,0.2,0.4],"laneChangeState":"laneChangeStarting","laneChangeDirection":"left"},"jetlink":{"source":"jetlink","lossLatched":true}}}"#,
    ));
    let event = fixture::project_event(&packet).unwrap();
    assert_eq!(
        event.get("logMonoTime").unwrap().to_json().unwrap(),
        "1250000000"
    );
    assert_eq!(
        event
            .get("modelV2")
            .unwrap()
            .get("meta")
            .unwrap()
            .get("laneChangeDirection")
            .unwrap()
            .to_utf8()
            .unwrap(),
        "left"
    );
    // Original pycapnp accepts these whole-valued floats for Int32 fields.
    let packet = bytes(&request(
        r#"{"op":"event","service":"carrotMan","time":1.25,"data":{"desiredSpeed":70.0,"vTurnSpeed":130.0}}"#,
    ));
    let carrot = fixture::project_event(&packet)
        .unwrap()
        .get("carrotMan")
        .unwrap();
    assert_eq!(carrot.get("desiredSpeed").unwrap().to_json().unwrap(), "70");
    assert_eq!(carrot.get("vTurnSpeed").unwrap().to_json().unwrap(), "130");
    for (value, expected) in [
        ("-20.0", "-20"),
        ("0.0", "0"),
        ("2147483647.0", "2147483647"),
        ("-2147483648.0", "-2147483648"),
    ] {
        let input = format!(
            r#"{{"op":"event","service":"carrotMan","time":1,"data":{{"desiredSpeed":{value}}}}}"#
        );
        let packet = bytes(&request(&input));
        assert_eq!(
            fixture::project_event(&packet)
                .unwrap()
                .get("carrotMan")
                .unwrap()
                .get("desiredSpeed")
                .unwrap()
                .to_json()
                .unwrap(),
            expected
        );
    }
    for value in [
        "70.5",
        "NaN",
        "Infinity",
        "-Infinity",
        "2147483648.0",
        "-2147483649.0",
        "1e100",
        "1e-100",
    ] {
        let input = format!(
            r#"{{"op":"event","service":"carrotMan","time":1,"data":{{"desiredSpeed":{value}}}}}"#
        );
        assert!(fixture::generate(&JsonValue::parse(&input).unwrap()).is_err());
    }
    for input in [
        r#"{"op":"event","service":"unimplementedService","time":1,"data":{}}"#,
        r#"{"op":"event","service":"carState","time":1,"data":{"noSuchField":1}}"#,
        r#"{"op":"event","service":"carState","time":1,"data":{"gearShifter":"noSuchEnum"}}"#,
    ] {
        assert!(fixture::generate(&JsonValue::parse(input).unwrap()).is_err());
    }
}

#[test]
fn psa_parameters_use_real_synthetic_car_and_apply_torque_changes() {
    use openpilot_cereal::car_capnp::car_params;
    let result = request(
        r#"{"op":"parameters","fingerprint":"PSA_PEUGEOT_208","mode":"torque","settings":{}}"#,
    );
    let wire = bytes(&result);
    let message =
        capnp::serialize::read_message(&wire[..], capnp::message::ReaderOptions::new()).unwrap();
    let cp = message.get_root::<car_params::Reader<'_>>().unwrap();
    assert_eq!(
        cp.get_car_fingerprint().unwrap().to_str().unwrap(),
        "PSA_PEUGEOT_208"
    );
    assert_eq!(cp.get_brand().unwrap().to_str().unwrap(), "psa");
    assert_eq!(cp.get_mass().to_bits(), 1600_f32.to_bits());
    assert!(cp.get_openpilot_longitudinal_control());
    let car_params::lateral_tuning::Torque(torque) = cp.get_lateral_tuning().which().unwrap()
    else {
        panic!("torque required")
    };
    assert_eq!(
        torque.unwrap().get_lat_accel_factor().to_bits(),
        2.5_f32.to_bits()
    );
}

#[test]
fn cli_rejects_existing_output_and_invalid_arguments() {
    let binary = env!("CARGO_BIN_EXE_generate-controlsd-fixtures");
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("requests.jsonl");
    let output = tmp.path().join("output.jsonl");
    fs::write(&input, b"{\"op\":\"frame\",\"index\":0}\n").unwrap();
    fs::write(&output, b"foreign").unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let result = Command::new(binary)
        .arg("--root")
        .arg(root)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(output).unwrap(), b"foreign");
    assert!(!Command::new(binary)
        .arg("--unknown")
        .arg("x")
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
#[ignore = "parent-owned exact-source Actions original/native corpus only"]
fn fresh_original_full_schema_and_float_bits_match_every_generated_packet() {
    fixture::audit_saved(&PathBuf::from(
        std::env::var_os("CONTROLSD_FIXTURE_PROOF").expect("Actions proof required"),
    ))
    .unwrap();
}
