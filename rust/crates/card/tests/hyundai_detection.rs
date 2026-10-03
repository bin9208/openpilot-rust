use openpilot_card::brands::hyundai::detection::{detect, DetectionInput};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    candidate: String,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    flags: u32,
    has_radar_dbc: bool,
    camera_scc: i32,
    hda2: i32,
    radar_tracks: i32,
    alpha_long: bool,
    result_flags: u32,
    ext_flags: u32,
    bsm: bool,
    radar_unavailable: bool,
    longitudinal: bool,
    safety_model: u16,
    safety_param: u16,
    bus: [u8; 3],
}

#[test]
fn capability_and_safety_when_all_source_identities_and_harness_variants() {
    let path = std::env::var("HYUNDAI_FIXTURE_DIR").expect("HYUNDAI_FIXTURE_DIR source fixture");
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(format!("{path}/detection.json")).unwrap()).unwrap();
    for case in &cases {
        let actual = detect(DetectionInput {
            candidate: &case.candidate,
            fingerprints: &case.fingerprints,
            flags: case.flags,
            has_radar_dbc: case.has_radar_dbc,
            camera_scc: case.camera_scc,
            hda2: case.hda2,
            radar_tracks: case.radar_tracks,
            alpha_long: case.alpha_long,
        });
        let expected = (
            case.result_flags,
            case.ext_flags,
            case.bsm,
            case.radar_unavailable,
            case.longitudinal,
            case.safety_model,
            case.safety_param,
            case.bus,
        );
        let observed = (
            actual.flags,
            actual.ext_flags,
            actual.bsm,
            actual.radar_unavailable,
            actual.longitudinal,
            u16::from(actual.safety_model),
            actual.safety_param,
            [actual.bus.ecan, actual.bus.acan, actual.bus.cam],
        );
        assert_eq!(
            observed, expected,
            "{} camera={} hda2={} radar={} bus={:?}",
            case.candidate, case.camera_scc, case.hda2, case.radar_tracks, case.bus
        );
    }
    std::fs::write(
        format!("{path}/native-detection.json"),
        serde_json::to_vec(
            &serde_json::json!({"matched": cases.len(), "different_harness_offsets":2}),
        )
        .unwrap(),
    )
    .unwrap();
}
