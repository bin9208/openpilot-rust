use super::*;
use std::path::Path;

fn controller() -> (tempfile::TempDir, Controller, State) {
    let temp = tempfile::tempdir().unwrap();
    let settings = Params::open(temp.path(), "d").unwrap();
    let cp = parameters(ParamsInput {
        candidate: "TESLA_MODEL_3",
        fingerprints: &[],
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    let cp = cp.get_root_as_reader::<car_params::Reader>().unwrap();
    let dbc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
    let state = State::new(
        &dbc,
        cp.get_flags(),
        f64::from(cp.get_steer_ratio()),
        cp.get_pcm_cruise(),
        0,
    )
    .unwrap();
    let controller = Controller::new(cp, &state, &settings).unwrap();
    (temp, controller, state)
}
fn input(speed: f64, requested: f64) -> SteeringInput {
    SteeringInput {
        requested,
        active: true,
        speed,
        raw_speed: speed,
        steering_angle: 0.,
        steering_rate: 80.,
        torque: 1.5,
    }
}
#[test]
fn inactive_feedback_matches_measured_can_and_holds_on_non_transmit_frame() {
    for (measured, rate, active, override_steering) in
        [(12.3, 80., false, false), (-15.6, -120., true, true)]
    {
        let (_temp, mut controller, mut state) = controller();
        let mut cs = state.out.get_root::<car_state::Builder>().unwrap();
        cs.set_v_ego(10.);
        cs.set_v_ego_raw(10.);
        cs.set_steering_angle_deg(measured);
        cs.set_steering_rate_deg(rate);
        state.extras.steering_disengage = override_steering;
        let mut cc = Message::new_default();
        let mut command = cc.init_root::<car_control::Builder>();
        command.set_lat_active(active);
        command.init_actuators().set_steering_angle_deg(30.);
        let first = controller
            .apply(
                &mut state,
                ApplyInput {
                    control: cc.get_root_as_reader().unwrap(),
                    now_ns: 0,
                    model: None,
                    radar: None,
                },
            )
            .unwrap();
        let feedback = first
            .actuators
            .get_root_as_reader::<car_control::actuators::Reader>()
            .unwrap()
            .get_steering_angle_deg();
        assert_eq!(feedback, measured);
        let sent = first.can.iter().find(|f| f.address == 0x488).unwrap();
        let message = controller
            .packer
            .dbc
            .message("DAS_steeringControl")
            .unwrap();
        let signal = message
            .signals
            .iter()
            .find(|s| s.name == "DAS_steeringAngleRequest")
            .unwrap();
        let wire = signal.physical(&signal.raw(&sent.data).unwrap()).unwrap();
        assert!((f64::from(feedback) + wire).abs() <= 0.051);
        cc.get_root::<car_control::Builder>()
            .unwrap()
            .get_actuators()
            .unwrap()
            .set_steering_angle_deg(-30.);
        let second = controller
            .apply(
                &mut state,
                ApplyInput {
                    control: cc.get_root_as_reader().unwrap(),
                    now_ns: 10_000_000,
                    model: None,
                    radar: None,
                },
            )
            .unwrap();
        assert!(second.can.iter().all(|f| f.address != 0x488));
        assert_eq!(
            second
                .actuators
                .get_root_as_reader::<car_control::actuators::Reader>()
                .unwrap()
                .get_steering_angle_deg(),
            feedback
        );
    }
}
#[test]
fn cooperative_override_is_cleared_at_float32_25_kph_boundary() {
    let (_temp, controller, _state) = controller();
    for speed in [(24.9_f64 / 3.6) as f32, (25.0_f64 / 3.6) as f32] {
        let mut coop = Coop {
            angle_override: 3.,
            ..Coop::default()
        };
        let speed = f64::from(speed);
        let expected = steering::limit(5., 0., speed, 0., true, &controller.vm).unwrap();
        let actual = coop.update(input(speed, 5.), &controller.vm).unwrap();
        assert_eq!(actual.to_bits(), expected.to_bits());
        assert_eq!(coop.angle_override, 0.);
        assert_eq!(coop.resume_rate_limiter_delta, 0.);
        assert_eq!(coop.resume_rate_limiter, actual);
        assert_eq!(coop.apply_angle_last, actual);
        assert_eq!(coop.coop_apply_angle_sat_last, actual);
    }
}
#[test]
fn speed_transition_limits_from_last_final_request_then_restarts_cooperation() {
    let (_temp, controller, _state) = controller();
    let mut coop = Coop::default();
    let mut previous = 0.;
    for _ in 0..6 {
        previous = coop.update(input(10., 5.), &controller.vm).unwrap();
    }
    let threshold = f64::from((25.0_f64 / 3.6) as f32);
    let expected = steering::limit(30., previous, threshold, 0., true, &controller.vm).unwrap();
    let actual = coop.update(input(threshold, 30.), &controller.vm).unwrap();
    assert_eq!(actual.to_bits(), expected.to_bits());
    assert_eq!(coop.angle_override, 0.);
    assert_eq!(coop.resume_rate_limiter_delta, 0.);
    let mut resumed = input(10., 10.);
    resumed.torque = 0.;
    coop.update(resumed, &controller.vm).unwrap();
    assert!(coop.resume_rate_limiter_delta > 0.);
}
