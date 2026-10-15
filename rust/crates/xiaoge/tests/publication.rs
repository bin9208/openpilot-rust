use openpilot_xiaoge::{
    config::Config,
    service::{state::Model, State},
    vision::{gate, GateInput, Side, BLINDSPOT_TIMEOUT_NS},
    wire,
};

#[test]
fn publication_preserves_independent_results_and_side_freshness() {
    let mut state = State::new(
        Config::default(),
        ["classifier", "lane"].map(|path| Model {
            path: path.to_owned(),
            loaded: true,
            error: String::new(),
        }),
        1.0,
    );
    state.cameras[0].last_frame = 10.0;
    state.cameras[0].error.clear();
    state.gate = gate(GateInput {
        alive_valid: true,
        left_width: 2.0,
        right_width: 3.0,
        configured: [true; 2],
        next_side: Side::Right,
    });
    state.blindspot_active = [true; 2];
    state.blindspot_side = Some(Side::Right);
    state.blindspot_updated = 10_000_000_000;
    state.blindspot_side_updated = [10_000_000_000 - BLINDSPOT_TIMEOUT_NS, 10_000_000_000];
    let publish = |state: &State, now| -> serde_json::Value {
        serde_json::from_slice(&wire::compact(&state.publication(10.0, now).unwrap()).unwrap())
            .unwrap()
    };
    let both = publish(&state, 10_000_000_000);
    assert_eq!(
        both["blindspot"]["validSides"],
        serde_json::json!(["left", "right"])
    );
    assert_eq!(both["blindspot"]["side"], "right");
    assert_eq!(both["blindspot"]["left"], true);
    let newer = publish(&state, 10_000_000_001);
    assert_eq!(
        newer["blindspot"]["validSides"],
        serde_json::json!(["right"])
    );
    assert_eq!(newer["blindspot"]["left"], false);
    assert_eq!(newer["blindspot"]["right"], true);
    assert_eq!(
        newer["blindspot"]["receivedMonoTimeNanosBySide"]["left"],
        state.blindspot_side_updated[0]
    );
    state.gate = gate(GateInput {
        alive_valid: true,
        left_width: 3.0,
        right_width: 1.0,
        configured: [true; 2],
        next_side: Side::Right,
    });
    let gated = publish(&state, 10_000_000_000);
    assert_eq!(
        gated["blindspot"]["validSides"],
        serde_json::json!(["left"])
    );
    assert_eq!(gated["blindspot"]["side"], "left");
    assert_eq!(gated["blindspot"]["right"], false);
}
