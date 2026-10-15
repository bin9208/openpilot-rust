use openpilot_ui_application::impact_prompt::{Policy, Visibility};

fn visible(now: f64) -> Visibility {
    Visibility {
        started: true,
        alive: true,
        valid: true,
        received: now,
        full: false,
        critical: false,
        rebooting: false,
    }
}
#[test]
fn cancellation_consumes_touch_and_new_token_rearms() {
    let mut policy = Policy::default();
    policy.notice(Some("one"));
    assert!(policy.touch(true, false, true));
    assert!(policy.present(1.0, visible(1.0)).is_none());
    policy.notice(Some("one"));
    assert!(policy.present(2.0, visible(2.0)).is_none());
    policy.notice(Some("two"));
    assert_eq!(policy.present(3.0, visible(3.0)), Some(10.0));
}
#[test]
fn invisible_notice_never_acknowledges_and_deadline_is_visible_time() {
    let mut policy = Policy::default();
    policy.notice(Some("one"));
    let mut gates = Vec::new();
    let mut input = visible(1.0);
    input.started = false;
    gates.push(input);
    let mut input = visible(1.0);
    input.alive = false;
    gates.push(input);
    let mut input = visible(1.0);
    input.valid = false;
    gates.push(input);
    let mut input = visible(1.0);
    input.full = true;
    gates.push(input);
    let mut input = visible(1.0);
    input.critical = true;
    gates.push(input);
    let mut input = visible(0.499999);
    input.received = 0.499999;
    gates.push(input);
    for gate in gates {
        assert_eq!(policy.present(1.0, gate), None);
    }
    assert_eq!(policy.present(5.0, visible(4.5)), Some(10.0));
    assert_eq!(policy.present(5.01, visible(5.01)), Some(10.0));
    assert_eq!(policy.present(6.01, visible(6.01)), Some(9.0));
    assert_eq!(policy.present(15.0, visible(15.0)), Some(0.0));
    assert!(!policy.touch(true, true, true));
}
