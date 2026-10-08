use openpilot_can::Packet;
use openpilot_radarcan::batch::{Batches, Ego, Reason, MAX_AGE_NS};

fn ego(first: u64, last: u64, count: u32, receive: u64, velocity: f64) -> Ego {
    Ego {
        first_can_ns: first,
        last_can_ns: last,
        packet_count: count,
        receive_ns: receive,
        v_ego: velocity,
        a_ego: -1.2,
    }
}
fn packets(times: impl IntoIterator<Item = u64>) -> Vec<Packet> {
    times
        .into_iter()
        .map(|mono_time| Packet {
            mono_time,
            frames: vec![],
        })
        .collect()
}

#[test]
fn partial_arrivals_keep_the_corresponding_ego_instead_of_the_latest_state() {
    let mut queue = Batches::default();
    queue.add_state(ego(10, 20, 2, 30, 12.5));
    queue.add_state(ego(40, 40, 1, 50, 33.5));
    assert!(queue.take(51).is_none());
    queue.add_can(packets([1, 10]));
    assert!(queue.take(51).is_none());
    queue.add_can(packets([20, 40, 80]));
    let first = queue.take(51).unwrap();
    assert_eq!(first.ego.v_ego, 12.5);
    assert_eq!(
        first
            .packets
            .iter()
            .map(|p| p.mono_time)
            .collect::<Vec<_>>(),
        [10, 20]
    );
    assert_eq!(first.error, None);
    let second = queue.take(51).unwrap();
    assert_eq!(second.ego.v_ego, 33.5);
    assert_eq!(second.packets[0].mono_time, 40);
    assert_eq!(queue.can[0].mono_time, 80);
}

#[test]
fn age_boundary_is_strict_and_future_metadata_does_not_underflow() {
    for (now, expected) in [
        (30 + MAX_AGE_NS, None),
        (31 + MAX_AGE_NS, Some(Reason::StaleEgoState)),
        (1, None),
    ] {
        let mut queue = Batches::default();
        queue.add_can(packets([10, 20]));
        queue.add_state(ego(10, 20, 2, 30, 12.5));
        assert_eq!(queue.take(now).unwrap().error, expected);
    }
}

#[test]
fn overflow_and_metadata_errors_preserve_source_precedence() {
    let mut queue = Batches::default();
    for _ in 0..33 {
        queue.add_state(ego(0, 0, 0, 0, 0.));
    }
    assert_eq!(queue.states.len(), 1);
    assert_eq!(
        queue.take(MAX_AGE_NS + 1).unwrap().error,
        Some(Reason::StateOverflow)
    );
    for (sample, expected) in [
        (ego(0, 0, 0, 0, 0.), Some(Reason::MissingBatchMetadata)),
        (ego(0, 0, 0, 30, 0.), None),
        (ego(10, 20, 0, 30, 0.), Some(Reason::InvalidEmptyBatch)),
        (ego(10, 20, 513, 30, 0.), Some(Reason::InvalidBatchMetadata)),
    ] {
        queue.add_state(sample);
        assert_eq!(queue.take(31).unwrap().error, expected);
    }
}

#[test]
fn packet_loss_consumes_only_the_failed_batch_and_capacity_drops_the_oldest_can() {
    let mut queue = Batches::default();
    queue.add_can(packets([10, 20, 40]));
    queue.add_state(ego(10, 20, 3, 30, 12.5));
    queue.add_state(ego(40, 40, 1, 50, 33.5));
    assert_eq!(
        queue.take(51).unwrap().error,
        Some(Reason::MissingCanPacket)
    );
    assert_eq!(queue.take(51).unwrap().error, None);
    queue.add_can(packets(1..523));
    assert_eq!(queue.can.len(), 512);
    queue.add_state(ego(1, 20, 20, 30, 12.5));
    assert_eq!(
        queue.take(31).unwrap().error,
        Some(Reason::MissingCanPacket)
    );
}
