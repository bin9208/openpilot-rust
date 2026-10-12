use super::*;
use std::time::{Duration, Instant};
use std::{sync::mpsc, thread};

#[test]
fn blocked_stats_close_returns_within_source_one_second_bound() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = RefreshWorker::new(
        "blocked-cluster-stats",
        1.0,
        SystemStats::default(),
        move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(SystemStats::default())
        },
    );
    let mut sampler = Sampler { worker };
    sampler.sample(0.0);
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let closer = thread::spawn(move || {
        let started = Instant::now();
        sampler.close();
        done_tx.send(started.elapsed()).unwrap();
    });
    let result = done_rx.recv_timeout(Duration::from_millis(1500));
    release_tx.send(()).unwrap();
    closer.join().unwrap();
    let elapsed = result.expect("stats close blocked past source timeout");
    assert!(elapsed >= Duration::from_millis(900), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
}
