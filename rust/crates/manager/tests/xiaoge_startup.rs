use openpilot_manager::xiaoge_startup::{Gate, Snapshot};

fn healthy(stamp: u64) -> Snapshot {
    Snapshot {
        times: [stamp; 5],
        valid: true,
        safety_ready: true,
        engageable: true,
        can_valid: true,
        can_timeout: false,
    }
}

#[test]
fn startup_requires_new_messages_and_half_second_continuity_each_onroad_session() {
    let mut gate = Gate::default();
    assert!(!gate.update(true, &healthy(100), true, 0.));
    assert!(!gate.update(true, &healthy(100), true, 1.));
    assert!(!gate.update(true, &healthy(101), true, 1.5));
    assert!(!gate.update(true, &healthy(102), true, 1.999));
    assert!(gate.update(true, &healthy(103), true, 2.));
    let mut bad = healthy(104);
    bad.can_valid = false;
    assert!(gate.update(true, &bad, false, 2.5));
    assert!(!gate.update(false, &healthy(0), true, 3.));
    assert!(!gate.update(true, &healthy(200), true, 4.));
    assert!(!gate.update(true, &healthy(201), true, 4.5));
    assert!(gate.update(true, &healthy(202), true, 5.));
}

#[test]
fn unhealthy_input_and_observation_gaps_reset_settling_without_cross_clock_comparison() {
    let mut gate = Gate::default();
    let mut snapshot = healthy(100);
    snapshot.times = [100, 10000, 5, 700, 9];
    assert!(!gate.update(true, &snapshot, true, 0.));
    snapshot.times = [101, 10001, 6, 701, 10];
    assert!(!gate.update(true, &snapshot, true, 0.5));
    assert!(!gate.update(true, &snapshot, true, 2.));
    for index in 0..5 {
        let mut bad = snapshot.clone();
        match index {
            0 => bad.valid = false,
            1 => bad.safety_ready = false,
            2 => bad.engageable = false,
            3 => bad.can_valid = false,
            _ => bad.can_timeout = true,
        }
        assert!(!gate.update(true, &bad, true, 2.1));
        assert!(!gate.update(true, &snapshot, true, 2.2));
    }
    assert!(!gate.update(true, &snapshot, false, 2.7));
    assert!(!gate.update(true, &snapshot, true, 3.));
    assert!(gate.update(true, &snapshot, true, 3.5));
}
