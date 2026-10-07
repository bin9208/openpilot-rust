use super::*;

fn memory(capacity: usize) -> Vec<AtomicU64> {
    (0..HEADER_WORDS + capacity / 8)
        .map(|_| AtomicU64::new(0))
        .collect()
}

#[test]
fn unaligned_payload_keeps_padding_and_returns_owned_bytes() {
    let words = memory(4096);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    let reader = queue.register(20, false, |_| {}).unwrap();
    words[HEADER_WORDS + 1].store(0x8877665544332211, SeqCst);

    queue.send(10, &[1, 2, 3], |_| {}).unwrap();
    let received = reader.receive(&queue).unwrap();

    assert_eq!(received, Read::Message(vec![1, 2, 3]));
    assert_eq!(words[HEADER_WORDS + 1].load(SeqCst), 0x8877665544030201);
    assert_eq!(words[1].load(SeqCst), 16);
    assert!(queue.readers_caught_up().unwrap());
    words[HEADER_WORDS + 1].store(0, SeqCst);
    assert_eq!(received, Read::Message(vec![1, 2, 3]));
}

#[test]
fn wrap_tag_advances_both_cycles_through_u32_rollover() {
    let words = memory(1024);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    words[1].store((u64::from(u32::MAX) << 32) | 896, SeqCst);
    let reader = queue.register(20, false, |_| {}).unwrap();

    queue.send(10, &[7; 120], |_| {}).unwrap();
    let received = reader.receive(&queue).unwrap();

    assert_eq!(words[HEADER_WORDS + 112].load(SeqCst), WRAP);
    assert_eq!(words[1].load(SeqCst), 128);
    assert_eq!(words[READ_POINTERS].load(SeqCst), 128);
    assert_eq!(received, Read::Message(vec![7; 120]));
}

#[test]
fn overwritten_reader_drops_old_messages_and_resumes_after_reset() {
    let words = memory(1024);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    let reader = queue.register(20, false, |_| {}).unwrap();
    for value in 0..8_u8 {
        queue.send(10, &[value; 120], |_| {}).unwrap();
    }

    let received = reader.receive(&queue).unwrap();

    assert_eq!(received, Read::Empty);
    assert_eq!(words[READ_POINTERS].load(SeqCst), (1_u64 << 32) | 128);
    queue.send(10, b"next", |_| {}).unwrap();
    assert_eq!(
        reader.receive(&queue).unwrap(),
        Read::Message(b"next".to_vec())
    );
}

#[test]
fn conflated_reader_returns_only_the_last_queued_payload() {
    let words = memory(1024);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    let reader = queue.register(20, true, |_| {}).unwrap();
    queue.send(10, b"first", |_| {}).unwrap();
    queue.send(10, b"second", |_| {}).unwrap();
    queue.send(10, b"last", |_| {}).unwrap();

    let received = reader.receive(&queue).unwrap();

    assert_eq!(received, Read::Message(b"last".to_vec()));
    assert_eq!(reader.ready(&queue).unwrap(), Ready::Empty);
}

#[test]
fn forty_first_registration_evicts_and_notifies_all_old_slots() {
    let words = memory(4096);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    let mut readers = Vec::new();
    for uid in 100..140_u64 {
        readers.push(queue.register(uid, false, |_| {}).unwrap());
    }
    let mut notifications = Vec::new();

    let new_reader = queue
        .register(999, false, |tid| notifications.push(tid))
        .unwrap();

    assert_eq!(notifications, (100..140).collect::<Vec<u32>>());
    assert_eq!(words[0].load(SeqCst), 1);
    assert!(readers
        .iter()
        .all(|reader| reader.ready(&queue).unwrap() == Ready::Evicted));
    queue.send(10, b"new reader", |_| {}).unwrap();
    assert_eq!(
        new_reader.receive(&queue).unwrap(),
        Read::Message(b"new reader".to_vec())
    );
}

#[test]
fn publisher_takeover_preserves_write_position_and_rejects_the_old_writer() {
    let words = memory(4096);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    let old_reader = queue.register(20, false, |_| {}).unwrap();
    queue.send(10, b"old", |_| {}).unwrap();

    queue.initialize_publisher(11);

    assert_eq!(words[1].load(SeqCst), 16);
    assert!(matches!(
        queue.send(10, b"stale", |_| {}),
        Err(Error::PublisherReplaced)
    ));
    assert_eq!(old_reader.receive(&queue).unwrap(), Read::Evicted);
    let reader = queue.register(21, false, |_| {}).unwrap();
    queue.send(11, b"new", |_| {}).unwrap();
    assert_eq!(
        reader.receive(&queue).unwrap(),
        Read::Message(b"new".to_vec())
    );
}

#[test]
fn invalid_length_tags_are_rejected_before_payload_access() {
    for tag in [0, u64::MAX - 1, 4096, 4097, u64::MAX] {
        let words = memory(4096);
        let queue = QueueMemory::new(&words).unwrap();
        queue.initialize_publisher(10);
        let reader = queue.register(20, false, |_| {}).unwrap();
        queue.send(10, b"ready", |_| {}).unwrap();
        words[HEADER_WORDS].store(tag, SeqCst);

        let result = reader.receive(&queue);

        assert!(matches!(result, Err(Error::Corrupt(_))), "tag={tag}");
    }
}

#[test]
fn malformed_header_offsets_and_counts_are_rejected() {
    let words = memory(4096);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    let reader = queue.register(20, false, |_| {}).unwrap();
    words[READ_POINTERS].store(3, SeqCst);

    let result = reader.receive(&queue);

    assert!(matches!(result, Err(Error::Corrupt(_))));
    words[0].store(41, SeqCst);
    assert!(matches!(
        queue.send(10, b"no", |_| {}),
        Err(Error::Corrupt(_))
    ));
    assert!(matches!(queue.readers_caught_up(), Err(Error::Corrupt(_))));
}

#[test]
fn concurrent_writer_and_reader_observe_whole_published_payloads() {
    let words = memory(8192);
    let queue = QueueMemory::new(&words).unwrap();
    queue.initialize_publisher(10);
    let reader = queue.register(20, false, |_| {}).unwrap();
    let count = if cfg!(miri) { 8 } else { 128 };

    std::thread::scope(|scope| {
        scope.spawn(|| {
            for value in 0..count {
                queue.send(10, &[value; 23], |_| {}).unwrap();
            }
        });
        for expected in 0..count {
            loop {
                match reader.receive(&queue).unwrap() {
                    Read::Message(bytes) => {
                        assert_eq!(bytes, [expected; 23]);
                        break;
                    }
                    Read::Empty => std::thread::yield_now(),
                    Read::Evicted => panic!("single reader was evicted"),
                }
            }
        }
    });

    assert!(queue.readers_caught_up().unwrap());
}
