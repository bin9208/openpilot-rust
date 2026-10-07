use openpilot_card::brands::hyundai::stopping::{CanfdStopping, StopInput, StopPhase};
use serde::Deserialize;

#[derive(Deserialize)]
struct Step {
    input: StopInput,
    expected: serde_json::Value,
}

#[test]
fn every_state_and_output_when_source_stopping_scenarios() {
    let path = std::env::var("HYUNDAI_FIXTURE_DIR").expect("HYUNDAI_FIXTURE_DIR source fixture");
    let scenarios: Vec<Vec<Step>> =
        serde_json::from_slice(&std::fs::read(format!("{path}/stopping.json")).unwrap()).unwrap();
    let mut count = 0;
    for (scenario, steps) in scenarios.iter().enumerate() {
        let mut controller = CanfdStopping::default();
        for (tick, step) in steps.iter().enumerate() {
            let command = controller.update(step.input);
            let actual = serde_json::json!({"controller":controller,"command":command});
            assert_eq!(actual, step.expected, "scenario={scenario} tick={tick}");
            count += 1;
        }
    }
    std::fs::write(
        format!("{path}/native-stopping.json"),
        serde_json::to_vec(&serde_json::json!({"matched_ticks":count,"scenarios":scenarios.len()}))
            .unwrap(),
    )
    .unwrap();
}

fn request() -> StopInput {
    StopInput {
        active: true,
        requested: true,
        speed: 0.3,
        held: false,
        accel: -0.5,
        value: -0.5,
        previous_value: -0.5,
        jerk_u: 2.,
        jerk_l: 1.,
    }
}

#[test]
fn bounded_reentry_when_persistent_creep() {
    let mut controller = CanfdStopping::default();
    let mut phases = Vec::new();
    for _ in 0..1400 {
        let command = controller.update(request()).unwrap();
        if phases.last() != Some(&controller.phase) {
            phases.push(controller.phase);
        }
        assert!(command.raw <= 0. && command.value <= 0.);
    }
    assert_eq!(
        phases,
        vec![
            StopPhase::Request,
            StopPhase::Release,
            StopPhase::Retry,
            StopPhase::Fallback
        ]
    );
    assert_eq!(controller.update(request()).unwrap().stop_req, 0);
}

#[test]
fn hold_retains_normal_acceleration_when_measured_stopped() {
    let mut controller = CanfdStopping::default();
    let input = StopInput {
        speed: 0.,
        held: true,
        accel: -1.,
        value: -0.3,
        previous_value: -0.3,
        ..request()
    };
    let command = controller.update(input).unwrap();
    assert_eq!(
        (command.stop_req, command.raw, command.value, command.lower),
        (1, -1., -0.3, 0.2)
    );
    assert_eq!(controller.phase, StopPhase::Held);
}

#[test]
fn stop_state_resets_when_inactive() {
    let mut controller = CanfdStopping::default();
    for _ in 0..200 {
        controller.update(request());
    }
    assert!(controller.retried);
    assert!(controller
        .update(StopInput {
            active: false,
            ..request()
        })
        .is_none());
    assert_eq!(controller.phase, StopPhase::Idle);
    assert!(!controller.retried);
}
