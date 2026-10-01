use openpilot_selfdrived::state::{EventType as E, State, StateMachine};

#[test]
fn soft_disable_expires_after_the_original_three_hundred_cycles() {
    let mut machine = StateMachine::default();
    assert!(machine.update(&[E::Enable]).active);
    machine.update(&[E::SoftDisable]);
    for _ in 0..299 {
        assert!(machine.update(&[E::SoftDisable]).active);
    }
    assert_eq!(machine.soft_disable_timer, 1);
    assert!(!machine.update(&[E::SoftDisable]).enabled);
    assert_eq!(machine.state, State::Disabled);
    assert_eq!(machine.current_alert_types, [E::Permanent]);
}

#[test]
fn user_disable_wins_when_immediate_and_soft_disable_arrive_together() {
    let mut machine = StateMachine::default();
    machine.update(&[E::Enable]);
    let flags = machine.update(&[E::ImmediateDisable, E::SoftDisable, E::UserDisable]);
    assert!(!flags.enabled && !flags.active);
    assert_eq!(machine.current_alert_types, [E::Permanent, E::UserDisable]);
}

#[test]
fn pre_enable_retains_priority_until_its_condition_clears() {
    let mut machine = StateMachine::default();
    let blocked = machine.update(&[E::Enable, E::NoEntry, E::PreEnable]);
    assert!(!blocked.enabled);
    assert_eq!(machine.current_alert_types, [E::Permanent, E::NoEntry]);
    let waiting = machine.update(&[E::Enable, E::PreEnable, E::SoftDisable]);
    assert!(waiting.enabled && !waiting.active);
    assert_eq!(machine.state, State::PreEnabled);
    assert!(machine.update(&[E::SoftDisable]).active);
    assert_eq!(machine.state, State::Enabled);
    machine.update(&[E::SoftDisable]);
    assert_eq!(machine.state, State::SoftDisabling);
}
