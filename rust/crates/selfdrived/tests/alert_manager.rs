use openpilot_selfdrived::alerts::{Alert, AlertManager, Priority};
use openpilot_selfdrived::state::EventType;

fn alert(name: &str, priority: Priority, duration: i64) -> Alert {
    Alert {
        alert_type: name.into(),
        event_type: Some(EventType::Warning),
        priority,
        duration,
        ..Alert::default()
    }
}

#[test]
fn repeated_alert_keeps_original_start_and_minimum_duration() {
    let mut manager = AlertManager::default();
    manager.add_many(10, [alert("a", Priority::Mid, 5)]);
    manager.add_many(11, [alert("a", Priority::Mid, 5)]);
    assert_eq!(manager.process_alerts(15, &[]).alert_type, "a");
    assert_eq!(manager.process_alerts(16, &[]), &Alert::default());
    manager.add_many(17, [alert("a", Priority::Mid, 5)]);
    assert_eq!(manager.process_alerts(22, &[]).alert_type, "a");
}

#[test]
fn newest_start_wins_but_equal_priority_and_start_keep_insertion_order() {
    let mut manager = AlertManager::default();
    manager.add_many(
        0,
        [alert("a", Priority::Low, 10), alert("b", Priority::Low, 10)],
    );
    assert_eq!(manager.process_alerts(0, &[]).alert_type, "a");
    manager.add_many(2, [alert("b", Priority::Low, 10)]);
    assert_eq!(manager.process_alerts(2, &[]).alert_type, "b");
    manager.add_many(3, [alert("c", Priority::Highest, 0)]);
    assert_eq!(manager.process_alerts(4, &[]).alert_type, "c");
    assert_eq!(manager.process_alerts(5, &[]).alert_type, "b");
}

#[test]
fn clear_expires_entries_and_readd_restarts_them() {
    let mut manager = AlertManager::default();
    manager.add_many(0, [alert("a", Priority::High, 50)]);
    assert_eq!(
        manager.process_alerts(1, &[EventType::Warning]),
        &Alert::default()
    );
    assert_eq!(manager.process_alerts(2, &[]), &Alert::default());
    manager.add_many(2, [alert("a", Priority::High, 50)]);
    assert_eq!(manager.process_alerts(52, &[]).alert_type, "a");
}
