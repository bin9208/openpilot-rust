use openpilot_proclogd::cadence::Cadence;
use std::time::Duration;

#[test]
fn deadlines_follow_each_publication_start() {
    let mut cadence = Cadence;
    assert_eq!(
        cadence.deadline(Duration::from_millis(700)),
        Duration::from_millis(2700)
    );
    assert_eq!(
        cadence.deadline(Duration::from_millis(2900)),
        Duration::from_millis(4900)
    );
}

#[test]
fn overrun_schedules_from_current_time_without_replaying_missed_slots() {
    let mut cadence = Cadence;
    assert_eq!(
        cadence.deadline(Duration::from_secs(1)),
        Duration::from_secs(3)
    );
    assert_eq!(
        cadence.deadline(Duration::from_secs(10)),
        Duration::from_secs(12)
    );
    assert_eq!(
        cadence.deadline(Duration::from_secs(12)),
        Duration::from_secs(14)
    );
}
