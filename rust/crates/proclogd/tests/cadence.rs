use openpilot_proclogd::cadence::Cadence;
use std::time::Duration;

#[test]
fn first_deadline_starts_after_collection_and_later_work_uses_absolute_deadline() {
    let mut cadence = Cadence::default();
    assert_eq!(
        cadence.deadline(Duration::from_millis(700)),
        Duration::from_millis(2700)
    );
    assert_eq!(
        cadence.deadline(Duration::from_millis(2900)),
        Duration::from_millis(4700)
    );
}

#[test]
fn overrun_keeps_accumulated_deadline_until_caught_up() {
    let mut cadence = Cadence::default();
    assert_eq!(
        cadence.deadline(Duration::from_secs(1)),
        Duration::from_secs(3)
    );
    assert_eq!(
        cadence.deadline(Duration::from_secs(10)),
        Duration::from_secs(5)
    );
    assert_eq!(
        cadence.deadline(Duration::from_secs(10)),
        Duration::from_secs(7)
    );
    assert_eq!(
        cadence.deadline(Duration::from_secs(10)),
        Duration::from_secs(9)
    );
    assert_eq!(
        cadence.deadline(Duration::from_secs(10)),
        Duration::from_secs(11)
    );
}
