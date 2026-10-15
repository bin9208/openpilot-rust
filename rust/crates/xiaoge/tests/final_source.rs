use openpilot_xiaoge::vision::{gate, Blindspot, GateInput, Side};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    source_commit: String,
    vasm_sha256: String,
    server_sha256: String,
    steps: Vec<Step>,
    gates: Vec<GateCase>,
}
#[derive(Deserialize)]
struct Step {
    side: Side,
    confidence: f64,
    threshold: f64,
    smoothing: f64,
    dt: f64,
    score: f64,
    active: bool,
}
#[derive(Deserialize)]
struct GateCase {
    input: GateInput,
    expected: serde_json::Value,
}
#[test]
fn original_classifier_ema_and_two_side_gate_fixture() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/final_source.json")).unwrap();
    assert_eq!(
        fixture.source_commit,
        "e8e152dbcfd9574a1791ed12026d1880e87689b3"
    );
    assert_eq!(
        fixture.vasm_sha256,
        "30e5492332265b43cb37d8d3caf46b2342571aa0356be9b0dd48a418385c0b52"
    );
    assert_eq!(
        fixture.server_sha256,
        "001ca4c0a3c2fb6d1f43971e5958dbe381459e41d33e089a3152c269acfdd78e"
    );
    let mut state = Blindspot::default();
    assert_eq!(fixture.steps.len(), 2048);
    for (index, step) in fixture.steps.into_iter().enumerate() {
        state.update(
            step.side,
            step.confidence,
            step.threshold,
            step.smoothing,
            step.dt,
        );
        assert_eq!(
            state.side(step.side).score.to_bits(),
            step.score.to_bits(),
            "EMA step {index}"
        );
        assert_eq!(
            state.side(step.side).active,
            step.active,
            "hysteresis step {index}"
        );
    }
    assert_eq!(fixture.gates.len(), 400);
    for case in fixture.gates {
        let actual = gate(case.input);
        let expected = case.expected;
        assert_eq!(serde_json::to_value(actual).unwrap(), expected);
    }
}
