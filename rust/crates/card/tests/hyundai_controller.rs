use openpilot_card::brands::hyundai::{ApplyInput, Hyundai, Setup};
use openpilot_cereal::{
    car_capnp::{car_control, car_state},
    log_capnp::{model_data_v2, radar_state},
};
use serde::Deserialize;
use std::{collections::BTreeMap, io::Write, path::PathBuf};

#[derive(Deserialize)]
struct StateCase {
    candidate: String,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    seed_now: u64,
    source_dir: PathBuf,
    steps: Vec<StateStep>,
}
#[derive(Deserialize)]
struct StateStep {
    now: u64,
    packets: Vec<openpilot_can::Packet>,
}
#[derive(Deserialize)]
struct Case {
    constructor: Vec<String>,
    state_case: usize,
    source_dir: PathBuf,
    settings: BTreeMap<String, i32>,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    diagnostics: Vec<String>,
    warnings: Vec<String>,
    soft_hold: i16,
    settings: BTreeMap<String, i32>,
    can: Vec<openpilot_can::Frame>,
}

#[test]
fn controller_when_model_override_settings_and_canfd_legacy_cadence() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let dbc = PathBuf::from(std::env::var("HYUNDAI_DBC_DIR").unwrap());
    let states: Vec<StateCase> =
        serde_json::from_slice(&std::fs::read(root.join("state.json")).unwrap()).unwrap();
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(root.join("controller.json")).unwrap()).unwrap();
    let native = root.join(format!("native-controller-{}", std::process::id()));
    std::fs::create_dir_all(&native).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let state = &states[case.state_case];
        let output = native.join(index.to_string());
        std::fs::create_dir_all(&output).unwrap();
        let params = openpilot_params::Params::open(&output.join("settings"), "p").unwrap();
        for (key, value) in &case.settings {
            params.put(key, value.to_string().as_bytes()).unwrap();
        }
        let bytes = std::fs::read(state.source_dir.join("params.bin")).unwrap();
        let mut vehicle = Hyundai::new(Setup {
            params_bytes: &bytes,
            dbc_root: &dbc,
            settings: params,
            fingerprints: &state.fingerprints,
            now_ns: state.seed_now,
        })
        .unwrap();
        assert_eq!(vehicle.state.config.candidate, state.candidate);
        assert_eq!(
            vehicle.take_diagnostics(),
            case.constructor,
            "constructor {}",
            state.candidate
        );
        let mut controls = std::fs::File::open(case.source_dir.join("control.bin")).unwrap();
        let mut models = std::fs::File::open(case.source_dir.join("model.bin")).unwrap();
        let mut radars = std::fs::File::open(case.source_dir.join("radar.bin")).unwrap();
        let mut actuator_output = std::fs::File::create(output.join("actuators.bin")).unwrap();
        let mut can_output = std::fs::File::create(output.join("can.jsonl")).unwrap();
        let mut diagnostics_output =
            std::fs::File::create(output.join("diagnostics.jsonl")).unwrap();
        for (tick, (step, input)) in case.steps.iter().zip(&state.steps).enumerate() {
            for (key, value) in &step.settings {
                vehicle
                    .state
                    .settings
                    .put(key, value.to_string().as_bytes())
                    .unwrap();
            }
            let mut cs = vehicle.update(&input.packets, input.now).unwrap();
            let mut out = cs.get_root::<car_state::Builder<'_>>().unwrap();
            out.set_lat_enabled(true);
            out.set_carrot_cruise(if (100..125).contains(&tick) { 1 } else { 0 });
            out.set_activate_cruise(if (20..30).contains(&tick) {
                -1
            } else if (10..20).contains(&tick) {
                1
            } else {
                0
            });
            out.set_steering_torque(if (80..110).contains(&tick) {
                290.
            } else if (180..205).contains(&tick) {
                -260.
            } else {
                0.
            });
            out.set_steering_pressed((90..105).contains(&tick) || (185..200).contains(&tick));
            out.set_steering_angle_deg(if (140..250).contains(&tick) {
                90.
            } else {
                -12.
            });
            vehicle.commit_state(out.into_reader()).unwrap();
            vehicle.set_soft_hold(step.soft_hold);
            let control =
                capnp::serialize::read_message(&mut controls, capnp::message::ReaderOptions::new())
                    .unwrap();
            let model =
                capnp::serialize::read_message(&mut models, capnp::message::ReaderOptions::new())
                    .unwrap();
            let radar =
                capnp::serialize::read_message(&mut radars, capnp::message::ReaderOptions::new())
                    .unwrap();
            let result = vehicle
                .apply(ApplyInput {
                    control: control.get_root::<car_control::Reader<'_>>().unwrap(),
                    now_ns: input.now,
                    model: Some(model.get_root::<model_data_v2::Reader<'_>>().unwrap()),
                    radar: Some(radar.get_root::<radar_state::Reader<'_>>().unwrap()),
                })
                .unwrap();
            let lines = vehicle.take_diagnostics();
            let warnings = vehicle.take_warnings();
            diagnostics_output
                .write_all(&serde_json::to_vec(&(&lines, &warnings)).unwrap())
                .unwrap();
            diagnostics_output.write_all(b"\n").unwrap();
            assert_eq!(
                lines, step.diagnostics,
                "{} prints tick {tick}",
                state.candidate
            );
            assert_eq!(
                warnings, step.warnings,
                "{} warnings tick {tick}",
                state.candidate
            );
            capnp::serialize::write_message(&mut actuator_output, &result.actuators).unwrap();
            can_output
                .write_all(&serde_json::to_vec(&result.can).unwrap())
                .unwrap();
            can_output.write_all(b"\n").unwrap();
            let actual: Vec<_> = result
                .can
                .iter()
                .map(|frame| (frame.address, &frame.data, frame.bus))
                .collect();
            let expected: Vec<_> = step
                .can
                .iter()
                .map(|frame| (frame.address, &frame.data, frame.bus))
                .collect();
            assert_eq!(actual, expected, "{} frame {tick}", state.candidate);
        }
    }
    std::fs::write(
        root.join("native-controller-path.txt"),
        native.to_string_lossy().as_bytes(),
    )
    .unwrap();
}
