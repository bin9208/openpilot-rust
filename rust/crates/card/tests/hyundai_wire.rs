use openpilot_can::Frame;
use openpilot_card::brands::hyundai::{
    canfd_steering::{self, SteeringInput, SteeringMessages},
    legacy_steering::{self, LkasInput},
    wire::{CanWriter, Values},
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

#[derive(Deserialize)]
struct FdCase {
    camera: bool,
    dbc: PathBuf,
    input: SteeringInput,
    source: SteeringMessages,
    expected: Vec<Frame>,
}

#[derive(Deserialize)]
struct LegacyCase {
    candidate: String,
    flags: u32,
    enabled: bool,
    warning: bool,
    ldws: bool,
    frame: u32,
    dbc: PathBuf,
    source: Values,
    expected: Frame,
}

fn fixture() -> PathBuf {
    PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").expect("HYUNDAI_FIXTURE_DIR source fixture"))
}

#[test]
fn canfd_steering_bytes_when_all_steering_and_harness_branches() {
    let root = fixture();
    let cases: Vec<FdCase> =
        serde_json::from_slice(&std::fs::read(root.join("steering.json")).unwrap()).unwrap();
    let mut actual = Vec::with_capacity(cases.len());
    let dbc = Arc::new(openpilot_can::dbc::Dbc::load(&cases[0].dbc).unwrap());
    for (index, case) in cases.iter().enumerate() {
        let mut writer = CanWriter::from_dbc(Arc::clone(&dbc));
        let frames = if case.camera {
            canfd_steering::camera_steering(&mut writer, &case.input, &case.source)
        } else {
            canfd_steering::steering(&mut writer, &case.input)
        }
        .unwrap();
        assert_eq!(
            serde_json::to_value(&frames).unwrap(),
            serde_json::to_value(&case.expected).unwrap(),
            "case={index}"
        );
        actual.push(frames);
    }
    std::fs::write(
        root.join("native-steering.json"),
        serde_json::to_vec(&actual).unwrap(),
    )
    .unwrap();
}

#[test]
fn legacy_steering_bytes_when_checksum_and_identity_variants() {
    let root = fixture();
    let cases: Vec<LegacyCase> =
        serde_json::from_slice(&std::fs::read(root.join("legacy-steering.json")).unwrap()).unwrap();
    let mut actual = Vec::with_capacity(cases.len());
    let dbc = Arc::new(openpilot_can::dbc::Dbc::load(&cases[0].dbc).unwrap());
    for (index, case) in cases.iter().enumerate() {
        let mut writer = CanWriter::from_dbc(Arc::clone(&dbc));
        let frame = legacy_steering::lkas(
            &mut writer,
            &LkasInput {
                candidate: &case.candidate,
                flags: case.flags,
                frame: case.frame,
                torque: -127.,
                steer_req: case.enabled,
                torque_fault: false,
                sys_warning: case.warning,
                sys_state: 3.,
                enabled: case.enabled,
                left_lane: true,
                right_lane: false,
                left_depart: 2.,
                right_depart: 0.,
                ldws_car: case.ldws,
            },
            &case.source,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&frame).unwrap(),
            serde_json::to_value(&case.expected).unwrap(),
            "case={index}"
        );
        actual.push(frame);
    }
    std::fs::write(
        root.join("native-legacy-steering.json"),
        serde_json::to_vec(&actual).unwrap(),
    )
    .unwrap();
}
