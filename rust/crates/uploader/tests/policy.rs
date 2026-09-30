use openpilot_uploader::{Backoff, Outcome};
#[test]
fn failed_upload_doubles_the_previous_idle_wait_and_caps_it() {
    // Given an offroad idle uploader.
    let mut backoff = Backoff::default();
    assert_eq!(backoff.next(Outcome::Idle, true, 0.0), 60.0);
    // When its next selected upload fails, then its previous wait participates in the cap.
    assert_eq!(backoff.next(Outcome::Failure, false, 0.5), 180.0);
}
#[test]
fn successful_upload_resets_backoff_to_original_point_one_second() {
    // Given retries that reached the cap.
    let mut backoff = Backoff::default();
    for _ in 0..20 {
        backoff.next(Outcome::Failure, false, 0.0);
    }
    // When an upload succeeds, then the next jittered delay is based on 0.1s.
    assert!((backoff.next(Outcome::Success, false, 0.5) - 0.15).abs() < 1e-15);
}
