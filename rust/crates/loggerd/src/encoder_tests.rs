use super::*;
use openpilot_cereal::log_capnp::encode_index::Type;
use std::{fs, io::Cursor, path::Path};

fn setup(stream: Stream, audio: bool) -> (tempfile::TempDir, Logger, Encoder) {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(&root.path().join("params"), "d").unwrap();
    params.put("RecordRoadCam", b"2").unwrap();
    params
        .put("RecordAudio", if audio { b"1" } else { b"0" })
        .unwrap();
    let encoder = Encoder::new(stream, &params).unwrap();
    let mut logger = Logger::new(root.path(), "route".into(), Vec::new());
    logger.next(10).unwrap();
    (root, logger, encoder)
}

fn packet(stream: Stream, segment: i32, frame: u8, keyframe: bool) -> Vec<u8> {
    packet_with_payload(stream, segment, frame, keyframe, &[frame])
}

fn packet_with_payload(
    stream: Stream,
    segment: i32,
    frame: u8,
    keyframe: bool,
    payload: &[u8],
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_valid(true);
    event.set_log_mono_time(u64::from(frame) * 50_000_000);
    let mut data = match stream {
        Stream::Road => event.init_road_encode_data(),
        Stream::Wide => event.init_wide_road_encode_data(),
        Stream::Driver => event.init_driver_encode_data(),
        Stream::Qroad => event.init_q_road_encode_data(),
    };
    data.set_width(160);
    data.set_height(120);
    data.set_data(payload);
    if keyframe {
        data.set_header(b"header");
    }
    let mut index = data.init_idx();
    index.set_type(Type::FullHEVC);
    index.set_frame_id(u32::from(frame));
    index.set_encode_id(u32::from(frame));
    index.set_segment_num(segment);
    index.set_segment_id(u32::from(frame));
    index.set_flags(if keyframe { 8 } else { 0 });
    index.set_len(u32::try_from(payload.len()).unwrap());
    index.set_timestamp_sof(u64::from(frame) * 50_000_000);
    index.set_timestamp_eof(u64::from(frame) * 50_000_000);
    capnp::serialize::write_message_to_words(&message)
}

fn logged_frames(path: &Path, stream: Stream) -> Vec<u32> {
    let bytes = zstd::decode_all(fs::File::open(path).unwrap()).unwrap();
    let mut reader = Cursor::new(bytes);
    let mut frames = Vec::new();
    while let Some(message) =
        capnp::serialize::try_read_message(&mut reader, Default::default()).unwrap()
    {
        let event = message.get_root::<event::Reader>().unwrap();
        match (stream, event.which().unwrap()) {
            (Stream::Road, event::RoadEncodeIdx(index))
            | (Stream::Qroad, event::QRoadEncodeIdx(index)) => {
                frames.push(index.unwrap().get_frame_id());
            }
            _ => {}
        }
    }
    frames
}

#[test]
fn immediate_writes_retain_receive_storage_and_match_fresh_buffer_output() {
    let (reused_root, mut reused_logger, mut reused_encoder) = setup(Stream::Road, false);
    let (fresh_root, mut fresh_logger, mut fresh_encoder) = setup(Stream::Road, false);
    let mut bytes = Vec::with_capacity(4096);
    let allocation = bytes.as_ptr();
    let capacity = bytes.capacity();
    for frame in 0..4 {
        let mut fresh = packet(Stream::Road, 70, frame, frame == 0);
        bytes.clear();
        bytes.extend_from_slice(&fresh);
        let reused = reused_encoder
            .handle(&mut reused_logger, &mut bytes, 0)
            .unwrap();
        let control = fresh_encoder
            .handle(&mut fresh_logger, &mut fresh, 0)
            .unwrap();
        assert_eq!(bytes.as_ptr(), allocation);
        assert_eq!(bytes.capacity(), capacity);
        assert_eq!(bytes, fresh);
        assert_eq!(reused.ready_part, None);
        assert_eq!(reused.bytes, control.bytes);
    }
    reused_encoder.close().unwrap();
    fresh_encoder.close().unwrap();
    reused_logger.close(0, 30).unwrap();
    fresh_logger.close(0, 30).unwrap();
    let media = fs::read(reused_root.path().join("route--0/fcamera.hevc")).unwrap();
    assert_eq!(media, b"header\x00\x01\x02\x03");
    assert_eq!(
        media,
        fs::read(fresh_root.path().join("route--0/fcamera.hevc")).unwrap()
    );
    for name in ["rlog.zst", "qlog.zst"] {
        let read = |root: &Path| {
            zstd::decode_all(fs::File::open(root.join("route--0").join(name)).unwrap()).unwrap()
        };
        assert_eq!(read(reused_root.path()), read(fresh_root.path()));
    }
}

#[test]
fn future_packets_own_compact_storage_and_replay_fifo_after_rotation() {
    let (root, mut logger, mut encoder) = setup(Stream::Road, false);
    encoder
        .handle(&mut logger, &mut packet(Stream::Road, 70, 0, true), 0)
        .unwrap();
    let expected = packet(Stream::Road, 71, 1, true);
    let mut bytes = Vec::with_capacity(2 * 1024 * 1024);
    bytes.extend_from_slice(&expected);
    let outcome = encoder.handle(&mut logger, &mut bytes, 0).unwrap();
    assert_eq!(outcome.ready_part, Some(1));
    assert!(bytes.is_empty());
    assert_eq!(bytes.capacity(), 0);
    assert_eq!(encoder.queue[0], expected);
    assert_eq!(encoder.queue[0].capacity(), expected.len());

    bytes.extend_from_slice(&packet(Stream::Road, 71, 2, false));
    let outcome = encoder.handle(&mut logger, &mut bytes, 1).unwrap();
    assert_eq!(outcome.ready_part, None);
    bytes.extend_from_slice(b"overwritten receive scratch");
    assert_eq!(encoder.queue[0], expected);
    assert_eq!(encoder.queue.len(), 2);
    logger.next(20).unwrap();
    bytes.clear();
    bytes.extend_from_slice(&packet(Stream::Road, 71, 3, false));
    encoder.handle(&mut logger, &mut bytes, 0).unwrap();
    assert!(encoder.queue.is_empty());
    encoder.close().unwrap();
    logger.close(0, 30).unwrap();
    assert_eq!(
        fs::read(root.path().join("route--1/fcamera.hevc")).unwrap(),
        b"header\x01\x02\x03"
    );
    assert_eq!(
        logged_frames(&root.path().join("route--1/rlog.zst"), Stream::Road),
        [1, 2, 3]
    );
}

#[test]
fn audio_waiting_packets_drain_only_on_the_next_video_packet() {
    // Raw output isolates the video queue's audio gate here; the native logger
    // comparison covers actual AAC/TS initialization and muxing.
    let (root, mut logger, mut encoder) = setup(Stream::Qroad, true);
    for frame in 0..2 {
        let mut bytes = packet(Stream::Qroad, 29, frame, frame == 0);
        assert_eq!(encoder.handle(&mut logger, &mut bytes, 0).unwrap().bytes, 0);
        assert!(bytes.is_empty());
    }
    encoder.audio_initialized = true;
    assert_eq!(encoder.queue.len(), 2);
    let mut bytes = packet(Stream::Qroad, 29, 2, false);
    let allocation = bytes.as_ptr();
    encoder.handle(&mut logger, &mut bytes, 0).unwrap();
    assert_eq!(bytes.as_ptr(), allocation);
    assert!(encoder.queue.is_empty());
    encoder.close().unwrap();
    logger.close(0, 30).unwrap();
    assert_eq!(
        fs::read(root.path().join("route--0/qcamera.ts")).unwrap(),
        b"header\x00\x01\x02"
    );
    assert_eq!(
        logged_frames(&root.path().join("route--0/rlog.zst"), Stream::Qroad),
        [0, 1, 2]
    );
}

#[test]
fn queue_admits_201_packets_and_retains_rejected_receive_storage() {
    for waiting_audio in [false, true] {
        let (_root, _logger, mut encoder) = setup(Stream::Road, false);
        for frame in 0_u32..201 {
            let mut bytes = frame.to_le_bytes().to_vec();
            encoder.enqueue(&mut bytes, waiting_audio);
            assert!(bytes.is_empty());
            assert_eq!(bytes.capacity(), 0);
        }
        let mut rejected = Vec::with_capacity(4096);
        rejected.extend_from_slice(b"overflow");
        let allocation = rejected.as_ptr();
        let capacity = rejected.capacity();
        encoder.enqueue(&mut rejected, waiting_audio);
        assert_eq!(encoder.queue.len(), 201);
        assert_eq!(encoder.queue[0], 0_u32.to_le_bytes());
        assert_eq!(encoder.queue[200], 200_u32.to_le_bytes());
        assert_eq!(rejected, b"overflow");
        assert_eq!(rejected.as_ptr(), allocation);
        assert_eq!(rejected.capacity(), capacity);
    }
}

#[test]
fn malformed_and_old_segment_packets_keep_storage_without_queueing() {
    let (_root, mut logger, mut encoder) = setup(Stream::Road, false);
    let mut bytes = vec![0xff; 64];
    let allocation = bytes.as_ptr();
    assert!(encoder.handle(&mut logger, &mut bytes, 0).is_err());
    assert_eq!(bytes.as_ptr(), allocation);
    encoder
        .handle(&mut logger, &mut packet(Stream::Road, 70, 0, true), 0)
        .unwrap();
    bytes.clear();
    bytes.extend_from_slice(&packet(Stream::Road, 69, 1, true));
    let allocation = bytes.as_ptr();
    let outcome = encoder.handle(&mut logger, &mut bytes, 0).unwrap();
    assert_eq!(outcome.ready_part, None);
    assert_eq!(outcome.bytes, 0);
    assert_eq!(bytes.as_ptr(), allocation);
    assert!(encoder.queue.is_empty());
    assert_eq!(encoder.offset, Some(0));
    encoder.close().unwrap();
    logger.close(0, 30).unwrap();
}

#[test]
fn oversized_packet_is_written_without_a_payload_limit() {
    let (root, mut logger, mut encoder) = setup(Stream::Road, false);
    let payload = vec![0x5a; 1024 * 1024 + 1];
    let mut bytes = packet_with_payload(Stream::Road, 70, 0, true, &payload);
    let allocation = bytes.as_ptr();
    let capacity = bytes.capacity();
    encoder.handle(&mut logger, &mut bytes, 0).unwrap();
    assert_eq!(bytes.as_ptr(), allocation);
    assert_eq!(bytes.capacity(), capacity);
    encoder.close().unwrap();
    logger.close(0, 30).unwrap();
    let media = fs::read(root.path().join("route--0/fcamera.hevc")).unwrap();
    assert_eq!(&media[..6], b"header");
    assert_eq!(&media[6..], payload);
    assert_eq!(
        logged_frames(&root.path().join("route--0/rlog.zst"), Stream::Road),
        [0]
    );
}

#[test]
fn write_error_keeps_receive_storage_without_queueing() {
    let (_root, mut logger, mut encoder) = setup(Stream::Road, false);
    encoder
        .handle(&mut logger, &mut packet(Stream::Road, 70, 0, true), 0)
        .unwrap();
    logger.close(0, 30).unwrap();
    let mut bytes = packet(Stream::Road, 70, 1, false);
    let allocation = bytes.as_ptr();
    let capacity = bytes.capacity();
    assert!(matches!(
        encoder.handle(&mut logger, &mut bytes, 0),
        Err(Error::Invalid("segment is closed"))
    ));
    assert_eq!(bytes.as_ptr(), allocation);
    assert_eq!(bytes.capacity(), capacity);
    assert!(encoder.queue.is_empty());
    encoder.close().unwrap();
}
