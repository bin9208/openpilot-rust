use openpilot_msgq::{MultiSubscriber, Publisher, Subscription};
use std::{env, process::Command, time::Duration};

#[test]
fn queued_poll_retains_every_packet_until_explicit_receive() {
    if env::var_os("RUST_MSGQ_QUEUED_CHILD").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-probe-queued-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let name = namespace
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "queued_poll_retains_every_packet_until_explicit_receive",
                "--nocapture",
            ])
            .env("RUST_MSGQ_QUEUED_CHILD", "1")
            .env("OPENPILOT_PREFIX", name)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let mut publisher = Publisher::new("queuedA").unwrap();
    let specifications = [
        Subscription {
            endpoint: "queuedA",
            capacity: 1024 * 1024,
            polled: true,
        },
        Subscription {
            endpoint: "queuedB",
            capacity: 1024 * 1024,
            polled: true,
        },
    ];
    let mut queues = MultiSubscriber::queued_for_runtime(&specifications).unwrap();
    assert_eq!(queues.receive_one(0).unwrap(), None);
    for value in 0_u32..1000 {
        publisher.send(&value.to_le_bytes()).unwrap();
    }

    let ready = queues.poll_ready(Duration::ZERO).unwrap();

    assert_eq!(ready, [0]);
    assert!(
        !publisher.readers_caught_up(),
        "poll must not dequeue the pending burst"
    );
    assert!(queues.receive_one(2).is_err());
    for value in 0_u32..1000 {
        assert_eq!(
            queues.receive_one(0).unwrap(),
            Some(value.to_le_bytes().to_vec())
        );
    }
    assert_eq!(queues.receive_one(0).unwrap(), None);
    assert!(publisher.readers_caught_up());

    drop(queues);
    let mut queues = MultiSubscriber::lazy_for_runtime(&specifications).unwrap();
    assert!(queues.receive_one(0).is_err());
    queues.set_active(0, true).unwrap();
    publisher.send(b"retained-while-other-opens").unwrap();
    queues.set_active(1, true).unwrap();
    queues.set_active(1, false).unwrap();
    assert_eq!(
        queues.receive_one(0).unwrap(),
        Some(b"retained-while-other-opens".to_vec())
    );
    queues.set_active(0, false).unwrap();
    assert!(queues.receive_one(0).is_err());
    assert!(queues.set_active(2, true).is_err());
    queues.set_active(0, true).unwrap();
    publisher.send(b"reconnected").unwrap();
    assert_eq!(queues.poll_ready(Duration::ZERO).unwrap(), [0]);
    assert_eq!(
        queues.receive_one(0).unwrap(),
        Some(b"reconnected".to_vec())
    );
}
