//! Ported from carrot-wip `opendbc/car/hyundai/tests/test_stopping.py`
//! (`test_stop_trial_preserves_real_lead...`, `test_receding_lead_stop_state...`).
use super::*;
use std::path::Path;

fn writer() -> CanWriter {
    CanWriter::new(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../opendbc_repo/opendbc/dbc/hyundai_canfd_generated.dbc"),
    )
    .unwrap()
}
/// A stock camera SCC frame with every signal at zero.
fn stock(writer: &CanWriter) -> Values {
    let message = writer.packer.dbc.message("SCC_CONTROL").unwrap();
    message
        .signals
        .iter()
        .map(|signal| (std::sync::Arc::<str>::from(signal.name.as_str()), 0.))
        .collect()
}
/// Stopped in drive, cruise available, nothing pressed.
fn state(original: &Values) -> SccState<'_> {
    SccState {
        original: Some(original),
        wheels: [0.; 4],
        v_ego: 0.,
        v_ego_raw: 0.,
        a_ego: 0.,
        brake: false,
        gas: false,
        brake_hold: false,
        parking_brake: false,
        can_valid: true,
        can_timeout: false,
        drive: true,
        available: true,
        standstill: false,
        paddle: 0,
        soft_hold: false,
        scc_hold: false,
    }
}
/// A stop request with a real lead 5.1 m ahead moving at `v_rel`.
fn input(v_rel: f64) -> AccInput {
    AccInput {
        bus: 0,
        enabled: true,
        accel_last: 0.,
        value_last: 0.,
        accel: -0.5,
        stopping: true,
        gas_override: false,
        set_speed: 30.,
        gap: 2.,
        jerk_u: 1.,
        jerk_l: 1.,
        carrot_cruise: 0,
        carrot_accel: 0.,
        lead: [5.1, -0.1, v_rel, if v_rel > 0.3 { 1. } else { 2. }],
        signal_stop_object: false,
        has_lead: true,
    }
}
fn signal(writer: &CanWriter, frame: &Frame, name: &str) -> f64 {
    let message = writer.packer.dbc.message("SCC_CONTROL").unwrap();
    let signal = message.signals.iter().find(|s| s.name == name).unwrap();
    signal.physical(&signal.raw(&frame.data).unwrap()).unwrap()
}
fn send(
    writer: &mut CanWriter,
    state: &SccState<'_>,
    controller: Option<&mut CanfdStopping>,
    input: &AccInput,
) -> Frame {
    camera_acc(writer, state, controller, input)
        .unwrap()
        .0
        .unwrap()
}

#[test]
fn final_stop_promotes_a_real_lead_and_release_restores_its_own_state() {
    for v_rel in [-0.4, 0., 0.3, 0.31, 0.6] {
        let mut writer = writer();
        let original = stock(&writer);
        let state = state(&original);
        let mut controller = CanfdStopping::default();
        let frame = send(&mut writer, &state, Some(&mut controller), &input(v_rel));
        assert_eq!(signal(&writer, &frame, "StopReq"), 1., "{v_rel}");
        assert_eq!(signal(&writer, &frame, "HUD_LEAD_INFO"), 2., "{v_rel}");
        assert!((signal(&writer, &frame, "ACC_ObjDist") - 5.1).abs() < 0.051);
        assert!((signal(&writer, &frame, "ACC_ObjRelSpd") - v_rel).abs() <= 0.051);
        let released = AccInput {
            stopping: false,
            ..input(v_rel)
        };
        let frame = send(&mut writer, &state, Some(&mut controller), &released);
        assert_eq!(signal(&writer, &frame, "StopReq"), 0., "{v_rel}");
        assert_eq!(
            signal(&writer, &frame, "HUD_LEAD_INFO"),
            if v_rel > 0.3 { 1. } else { 2. },
            "{v_rel}"
        );
    }
}

#[test]
fn receding_lead_stop_state_follows_final_admission() {
    for scenario in [
        "approach",
        "disabled",
        "override",
        "brake",
        "gas",
        "can_invalid",
        "reverse",
        "legacy",
        "soft_hold",
    ] {
        let mut writer = writer();
        let original = stock(&writer);
        let mut state = state(&original);
        let mut input = input(0.6);
        let mut controller = Some(CanfdStopping::default());
        match scenario {
            "approach" => {
                state.v_ego = 1.;
                state.v_ego_raw = 1.;
                state.wheels = [1.; 4];
            }
            "disabled" => input.enabled = false,
            "override" => input.gas_override = true,
            "brake" => state.brake = true,
            "gas" => state.gas = true,
            "can_invalid" => state.can_valid = false,
            "reverse" => state.drive = false,
            "legacy" => controller = None,
            _ => {
                state.soft_hold = true;
                input.enabled = false;
                input.stopping = false;
            }
        }
        let frame = send(&mut writer, &state, controller.as_mut(), &input);
        assert_eq!(
            signal(&writer, &frame, "HUD_LEAD_INFO"),
            if scenario == "soft_hold" { 2. } else { 1. },
            "{scenario}"
        );
        assert!((signal(&writer, &frame, "ACC_ObjDist") - 5.1).abs() < 0.051);
        assert!((signal(&writer, &frame, "ACC_ObjRelSpd") - 0.6).abs() <= 0.051);
    }
}
