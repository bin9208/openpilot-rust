use openpilot_cluster::h264::native_sender::{select_access_units, AccessUnit};
use openpilot_cluster::{
    h264::{native_sender::Sender, Control, Output},
    usb::H264Chunk,
};
use std::{
    io,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

fn unit(value: u8, codec_config: bool, keyframe: bool) -> AccessUnit {
    AccessUnit {
        chunks: vec![vec![value], vec![value, 1]],
        codec_config,
        keyframe,
    }
}
#[test]
fn latest_codec_keyframe_and_incoming_units_survive_in_original_order() {
    let selected = select_access_units(
        vec![
            unit(0, true, false),
            unit(1, false, true),
            unit(2, false, false),
        ],
        unit(3, false, false),
    );
    assert_eq!(
        selected
            .kept
            .iter()
            .map(|unit| unit.chunks[0][0])
            .collect::<Vec<_>>(),
        [0, 1, 3]
    );
    assert_eq!((selected.dropped_units, selected.dropped_chunks), (1, 2));
    let selected = select_access_units(selected.kept, unit(4, false, true));
    assert_eq!(
        selected
            .kept
            .iter()
            .map(|unit| unit.chunks[0][0])
            .collect::<Vec<_>>(),
        [0, 4]
    );
    assert_eq!((selected.dropped_units, selected.dropped_chunks), (2, 4));
}
#[test]
fn repeated_payloads_are_selected_by_unit_identity_and_flags() {
    let selected = select_access_units(
        vec![unit(1, true, false), unit(1, false, true)],
        unit(1, false, false),
    );
    assert_eq!(selected.kept.len(), 3);
    assert_eq!((selected.dropped_units, selected.dropped_chunks), (0, 0));
    assert!(selected.kept[0].codec_config);
    assert!(selected.kept[1].keyframe);
    assert!(!selected.kept[2].keyframe);
}

struct OwnedOutput {
    records: Arc<Mutex<Vec<String>>>,
    entered: Option<mpsc::SyncSender<()>>,
    permit: mpsc::Receiver<()>,
    fail: bool,
}
impl Output for OwnedOutput {
    fn start(&mut self, requested: i64) -> io::Result<usize> {
        self.records
            .lock()
            .unwrap()
            .push(format!("start:{requested}"));
        Ok(32)
    }
    fn send(&mut self, chunk: H264Chunk<'_>) -> io::Result<()> {
        self.records.lock().unwrap().push(format!(
            "send:{:?}:{}:{}:{}",
            chunk.bytes, chunk.last, chunk.wait_for_ack, chunk.require_ack
        ));
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.permit.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        if self.fail {
            return Err(io::Error::other("owned controlled transfer failure"));
        }
        Ok(())
    }
    fn control(&mut self, control: Control) -> io::Result<()> {
        self.records
            .lock()
            .unwrap()
            .push(format!("control:{control:?}"));
        Ok(())
    }
    fn stop(&mut self) {
        self.records.lock().unwrap().push("stop".into());
    }
}

#[test]
fn actual_sender_retains_latest_units_preserves_ack_flags_and_closes_its_output() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let output_records = Arc::clone(&records);
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, permit) = mpsc::channel();
    let mut sender = Sender::start(64, true, true, move || {
        Ok(OwnedOutput {
            records: output_records,
            entered: Some(entered),
            permit,
            fail: false,
        })
    })
    .unwrap();
    assert_eq!(sender.chunk_size, 32);
    sender.enqueue(unit(9, false, true)).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    for unit in [
        unit(0, true, false),
        unit(1, false, true),
        unit(2, false, false),
        unit(3, false, false),
    ] {
        sender.enqueue(unit).unwrap();
    }
    let snapshot = sender.snapshot().unwrap();
    assert_eq!((snapshot.dropped_units, snapshot.dropped_chunks), (1, 2));
    release.send(()).unwrap();
    sender.control(Control::Brightness(19)).unwrap();
    // Closing now cancels queued work. Finish this delivery assertion before
    // exercising close, rather than depending on its former implicit drain.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while sender.snapshot().unwrap().chunks_sent < 8 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    sender.close().unwrap();
    sender.close().unwrap();
    let snapshot = sender.snapshot().unwrap();
    assert_eq!((snapshot.bytes_sent, snapshot.chunks_sent), (12, 8));
    assert!(snapshot.error.is_none());
    let records = records.lock().unwrap();
    assert_eq!(records.first().unwrap(), "start:64");
    assert_eq!(records.last().unwrap(), "stop");
    assert_eq!(
        records
            .iter()
            .filter(|record| record.as_str() == "stop")
            .count(),
        1
    );
    assert!(records
        .iter()
        .filter(|record| record.starts_with("send:"))
        .all(|record| record.ends_with(":false:true:false")));
    assert!(!records.iter().any(|record| record.starts_with("send:[2")));
    println!("owned native sender: {records:?}; {snapshot:?}");
}

#[test]
fn actual_sender_transfer_failure_is_observable_before_repeated_close() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let output_records = Arc::clone(&records);
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, permit) = mpsc::channel();
    let mut sender = Sender::start(0, false, false, move || {
        Ok(OwnedOutput {
            records: output_records,
            entered: Some(entered),
            permit,
            fail: true,
        })
    })
    .unwrap();
    sender.enqueue(unit(1, false, true)).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while sender.snapshot().unwrap().error.is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(sender
        .check_error()
        .unwrap_err()
        .to_string()
        .contains("controlled transfer failure"));
    assert!(sender.control(Control::Brightness(19)).is_err());
    sender.close().unwrap();
    sender.close().unwrap();
    assert_eq!(records.lock().unwrap().last().unwrap(), "stop");
    println!(
        "owned native sender failure: {:?}",
        sender.snapshot().unwrap()
    );
}

#[test]
fn closing_cancels_pending_frames_and_remaining_chunks_before_output_stop() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let output_records = Arc::clone(&records);
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, permit) = mpsc::channel();
    let mut sender = Sender::start(0, false, false, move || {
        Ok(OwnedOutput {
            records: output_records,
            entered: Some(entered),
            permit,
            fail: false,
        })
    })
    .unwrap();
    sender.enqueue(unit(1, false, true)).unwrap();
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    sender.enqueue(unit(2, false, false)).unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        release.send(()).unwrap();
    });
    sender.close().unwrap();
    release.join().unwrap();
    let records = records.lock().unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|line| line.starts_with("send:"))
            .count(),
        1
    );
    assert_eq!(records.last().unwrap(), "stop");
}
