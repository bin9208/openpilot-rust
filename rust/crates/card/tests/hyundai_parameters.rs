use openpilot_card::brands::hyundai::{Hyundai, ParamsInput};
use openpilot_params::Params;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Case {
    candidate: String,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    camera_scc: i32,
    hda2: i32,
    radar_tracks: i32,
    alpha_long: bool,
    diagnostics: Vec<String>,
}

#[test]
fn complete_schema_parameters_when_source_matrix() {
    let fixture = PathBuf::from(
        std::env::var("HYUNDAI_FIXTURE_DIR").expect("HYUNDAI_FIXTURE_DIR source fixture"),
    );
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(fixture.join("detection.json")).unwrap()).unwrap();
    let output = fixture.join(format!("native-params-{}", std::process::id()));
    std::fs::create_dir_all(&output).unwrap();
    let settings = Params::open(&output.join("settings"), "p").unwrap();
    for (index, case) in cases.iter().enumerate() {
        for (key, value) in [
            ("HyundaiCameraSCC", case.camera_scc),
            ("CanfdHDA2", case.hda2),
            ("EnableRadarTracks", case.radar_tracks),
        ] {
            settings.put(key, value.to_string().as_bytes()).unwrap();
        }
        let input = ParamsInput {
            candidate: &case.candidate,
            fingerprints: &case.fingerprints,
            firmware: &[],
            alpha_long: case.alpha_long,
            is_release: false,
            settings: &settings,
        };
        let result = Hyundai::parameters(input).unwrap();
        let diagnostics = Hyundai::parameter_diagnostics(&input, &result).unwrap();
        std::fs::write(
            output.join(format!("{index}-diagnostics.json")),
            serde_json::to_vec(&diagnostics).unwrap(),
        )
        .unwrap();
        assert_eq!(
            diagnostics, case.diagnostics,
            "{} case {index}",
            case.candidate
        );
        std::fs::write(
            output.join(format!("{index}.bin")),
            capnp::serialize::write_message_to_words(&result),
        )
        .unwrap();
    }
    assert!(matches!(
        Hyundai::parameters(ParamsInput {
            candidate: "KIA_K5_DL3_24_HEV",
            fingerprints: &[],
            firmware: &[],
            alpha_long: false,
            is_release: false,
            settings: &settings
        }),
        Err(openpilot_card::brands::hyundai::Error::Baseline(
            openpilot_card::vehicle_params::Error::MissingTorque(_)
        ))
    ));
    std::fs::write(
        fixture.join("native-params-path.txt"),
        output.to_string_lossy().as_bytes(),
    )
    .unwrap();
}
