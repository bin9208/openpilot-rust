use openpilot_card::brands::hyundai::{
    canfd_acc::{self, AccInput, SccState},
    legacy_acc::{self, LegacyAccInput, LegacySccMessages},
    stopping::{CanfdStopping, StopPhase},
    wire::{CanWriter, Values},
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Deserialize)]
struct FdCase {
    camera: bool,
    control: bool,
    dbc: PathBuf,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    input: AccInput,
    state: State,
    expected: Expected,
}
#[derive(Deserialize, serde::Serialize)]
struct Expected {
    frame: openpilot_can::Frame,
    value: f64,
    phase: Option<StopPhase>,
}
#[derive(Deserialize)]
struct State {
    original: Option<Values>,
    wheels: [f64; 4],
    v_ego: f64,
    v_ego_raw: f64,
    a_ego: f64,
    brake: bool,
    gas: bool,
    brake_hold: bool,
    parking_brake: bool,
    can_valid: bool,
    drive: bool,
    available: bool,
    standstill: bool,
    paddle: u8,
    soft_hold: bool,
    scc_hold: bool,
}
impl State {
    fn reader(&self) -> SccState<'_> {
        SccState {
            original: self.original.as_ref(),
            wheels: self.wheels,
            v_ego: self.v_ego,
            v_ego_raw: self.v_ego_raw,
            a_ego: self.a_ego,
            brake: self.brake,
            gas: self.gas,
            brake_hold: self.brake_hold,
            parking_brake: self.parking_brake,
            can_valid: self.can_valid,
            drive: self.drive,
            available: self.available,
            standstill: self.standstill,
            paddle: self.paddle,
            soft_hold: self.soft_hold,
            scc_hold: self.scc_hold,
        }
    }
}
#[derive(Deserialize)]
struct LegacyCase {
    camera: bool,
    dbc: PathBuf,
    input: LegacyAccInput,
    source: BTreeMap<String, Values>,
    expected: serde_json::Value,
}

#[test]
fn canfd_scc_bytes_and_recovery_when_pedals_holds_and_enable_variants() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let cases: Vec<FdCase> =
        serde_json::from_slice(&std::fs::read(root.join("acc.json")).unwrap()).unwrap();
    let dbc = Arc::new(openpilot_can::dbc::Dbc::load(&cases[0].dbc).unwrap());
    let mut outputs = Vec::new();
    for (case_index, case) in cases.iter().enumerate() {
        let mut writer = CanWriter::from_dbc(Arc::clone(&dbc));
        let mut stopping = CanfdStopping::default();
        let mut trace = Vec::new();
        for (tick, step) in case.steps.iter().enumerate() {
            let controller = if case.control {
                Some(&mut stopping)
            } else {
                None
            };
            let (frame, value) = if case.camera {
                let (frame, value) = canfd_acc::camera_acc(
                    &mut writer,
                    &step.state.reader(),
                    controller,
                    &step.input,
                )
                .unwrap();
                (frame.unwrap(), value)
            } else {
                canfd_acc::acc(&mut writer, &step.state.reader(), controller, &step.input).unwrap()
            };
            let actual = serde_json::json!({"frame":frame,"value":value,
                "phase":if case.control {Some(stopping.phase)} else {None}});
            assert_eq!(
                actual,
                serde_json::to_value(&step.expected).unwrap(),
                "case={case_index} tick={tick}"
            );
            trace.push(actual);
        }
        outputs.push(trace);
    }
    std::fs::write(
        root.join("native-acc.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}

#[test]
fn legacy_scc_bytes_when_camera_and_longitudinal_variants() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let cases: Vec<LegacyCase> =
        serde_json::from_slice(&std::fs::read(root.join("legacy-acc.json")).unwrap()).unwrap();
    let dbc = Arc::new(openpilot_can::dbc::Dbc::load(&cases[0].dbc).unwrap());
    let mut outputs = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let mut writer = CanWriter::from_dbc(Arc::clone(&dbc));
        let original = if case.camera {
            Some(LegacySccMessages {
                scc11: case.source.get("SCC11"),
                scc12: case.source.get("SCC12"),
                scc14: case.source.get("SCC14"),
                fca11: case.source.get("FCA11"),
            })
        } else {
            None
        };
        let frames = legacy_acc::commands(&mut writer, &case.input, original).unwrap();
        let actual = serde_json::to_value(&frames).unwrap();
        assert_eq!(actual, case.expected, "case={index}");
        outputs.push(actual);
    }
    std::fs::write(
        root.join("native-legacy-acc.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}
