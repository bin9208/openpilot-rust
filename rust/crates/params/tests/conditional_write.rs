use openpilot_params::Params;
use std::sync::{Arc, Barrier};

#[test]
fn stale_snapshots_cannot_replace_newer_writes() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params.put("VEgoStopping", b"NaN").unwrap();
    let observed = params.get("VEgoStopping").unwrap();
    params.put("VEgoStopping", b"75").unwrap();
    assert!(!params
        .put_if_unchanged("VEgoStopping", observed.as_deref(), b"50")
        .unwrap());
    assert_eq!(params.get("VEgoStopping").unwrap(), Some(b"75".to_vec()));
    assert!(params
        .put_if_unchanged("VEgoStopping", Some(b"75"), b"90")
        .unwrap());
    assert_eq!(params.get("VEgoStopping").unwrap(), Some(b"90".to_vec()));
}

#[test]
fn missing_and_empty_values_have_different_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    assert!(!params
        .put_if_unchanged("VEgoStopping", Some(b""), b"10")
        .unwrap());
    assert!(params.put_if_unchanged("VEgoStopping", None, b"").unwrap());
    assert!(!params
        .put_if_unchanged("VEgoStopping", None, b"50")
        .unwrap());
    assert!(params
        .put_if_unchanged("VEgoStopping", Some(b""), b"50")
        .unwrap());
    assert!(params.put_if_unchanged("Unknown", None, b"50").is_err());
}

#[test]
fn concurrent_writers_cannot_both_replace_the_same_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params.put("VEgoStopping", b"50").unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let writers = [b"75", b"90"].map(|value| {
        let params = params.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            params
                .put_if_unchanged("VEgoStopping", Some(b"50"), value)
                .unwrap()
        })
    });
    barrier.wait();
    let successes = writers
        .into_iter()
        .map(|writer| usize::from(writer.join().unwrap()))
        .sum::<usize>();
    assert_eq!(successes, 1);
    assert!(matches!(
        params.get("VEgoStopping").unwrap().as_deref(),
        Some(b"75" | b"90")
    ));
}
