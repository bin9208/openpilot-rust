use openpilot_card::brands::hyundai::{
    config::CarConfig,
    state::{CanSetup, State},
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
struct Case {
    candidate: String,
    settings: BTreeMap<String, i32>,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    seed_now: u64,
    source_dir: PathBuf,
    steps: Vec<Step>,
    constructor: Vec<String>,
}
#[derive(Deserialize)]
struct Step {
    now: u64,
    packets: Vec<openpilot_can::Packet>,
    diagnostics: Vec<String>,
    warnings: Vec<String>,
}

#[test]
fn complete_car_state_when_legacy_canfd_capture_and_timeout_streams() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let dbc = PathBuf::from(std::env::var("HYUNDAI_DBC_DIR").unwrap());
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(root.join("state.json")).unwrap()).unwrap();
    let native = root.join(format!("native-state-{}", std::process::id()));
    std::fs::create_dir_all(&native).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let output = native.join(index.to_string());
        std::fs::create_dir_all(&output).unwrap();
        let settings = openpilot_params::Params::open(&output.join("settings"), "p").unwrap();
        for (key, value) in &case.settings {
            settings.put(key, value.to_string().as_bytes()).unwrap();
        }
        let bytes = std::fs::read(case.source_dir.join("params.bin")).unwrap();
        let config = CarConfig::decode(&bytes, &settings).unwrap();
        assert_eq!(config.candidate, case.candidate);
        let filename = if config.flags & 8192 != 0 {
            "hyundai_canfd_generated.dbc"
        } else {
            "hyundai_kia_generic.dbc"
        };
        let mut state = State::new(
            config,
            settings,
            CanSetup {
                path: &dbc.join(filename),
                fingerprints: &case.fingerprints,
                now_ns: case.seed_now,
            },
        )
        .unwrap();
        assert_eq!(
            std::mem::take(&mut state.inputs.diagnostics.prints),
            case.constructor,
            "constructor {}",
            case.candidate
        );
        let mut states_output = std::fs::File::create(output.join("states.bin")).unwrap();
        let mut diagnostics_output =
            std::fs::File::create(output.join("diagnostics.jsonl")).unwrap();
        for (tick, step) in case.steps.iter().enumerate() {
            let result = state.update(&step.packets, step.now).unwrap();
            capnp::serialize::write_message(&mut states_output, &result).unwrap();
            let lines = std::mem::take(&mut state.inputs.diagnostics.prints);
            let warnings = std::mem::take(&mut state.inputs.diagnostics.warnings);
            use std::io::Write;
            diagnostics_output
                .write_all(&serde_json::to_vec(&(&lines, &warnings)).unwrap())
                .unwrap();
            diagnostics_output.write_all(b"\n").unwrap();
            assert_eq!(
                lines, step.diagnostics,
                "{} prints tick {tick}",
                case.candidate
            );
            assert_eq!(
                warnings, step.warnings,
                "{} warnings tick {tick}",
                case.candidate
            );
        }
    }
    std::fs::write(
        root.join("native-state-path.txt"),
        native.to_string_lossy().as_bytes(),
    )
    .unwrap();
}
