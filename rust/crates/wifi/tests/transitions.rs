use openpilot_wifi::{state::State, transition::Signal, ConnectStatus, Event, WifiState};

#[test]
fn delayed_activation_cannot_replace_a_newer_user_connection() {
    let mut state = State::new(None);
    state.new_connection("A".into(), "/A".into());
    state.new_connection("B".into(), "/B".into());
    state.set_connecting(Some("A".into()));
    let pending = state
        .begin_transition(Signal {
            current: 100,
            previous: 90,
            reason: 0,
        })
        .unwrap();
    state.set_connecting(Some("B".into()));
    assert!(!state.finish_transition(pending, Some("/A")));
    assert_eq!(state.snapshot().connecting_to_ssid.as_deref(), Some("B"));
    assert!(state.events.is_empty());
}

#[test]
fn forgetting_a_different_network_preserves_new_connection() {
    let mut state = State::new(None);
    state.new_connection("B".into(), "/B".into());
    state.set_connecting(Some("B".into()));
    state.begin_transition(Signal {
        current: 30,
        previous: 110,
        reason: 38,
    });
    assert_eq!(state.snapshot().connecting_to_ssid.as_deref(), Some("B"));
    state.remove_connection("/B");
    state.begin_transition(Signal {
        current: 30,
        previous: 110,
        reason: 38,
    });
    assert_eq!(state.snapshot.wifi_state, WifiState::default());
}

#[test]
fn stale_auth_signal_does_not_open_password_dialog() {
    let mut state = State::new(None);
    state.set_connecting(Some("B".into()));
    state.begin_transition(Signal {
        current: 60,
        previous: 30,
        reason: 8,
    });
    assert!(state.events.is_empty());
    state.begin_transition(Signal {
        current: 60,
        previous: 50,
        reason: 8,
    });
    assert_eq!(state.events, [Event::NeedAuth("B".into())]);
    assert_eq!(
        state.snapshot.wifi_state.status,
        ConnectStatus::Disconnected
    );
}

#[test]
fn scanner_keeps_background_connected_quiet_and_uses_strict_boundary() {
    let mut state = State::new(None);
    state.last_scan = 100.;
    state.active = false;
    assert!(!state.scan_due(105.));
    assert!(state.scan_due(105.1));
    state.snapshot.wifi_state.status = ConnectStatus::Connected;
    assert!(!state.scan_due(160.));
    state.active = true;
    assert!(state.scan_due(160.));
}
