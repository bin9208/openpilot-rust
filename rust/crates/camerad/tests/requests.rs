use openpilot_camerad::requests::{CameraRequests, FrameEvent, RequestIo, StressPoint};
use openpilot_camerad::startup::FrameSync;

#[derive(Default)]
struct Io {
    calls: Vec<String>,
    fail_wait: bool,
    fail_stress: bool,
    now: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid stress probability")]
struct StressError;

impl RequestIo for Io {
    type Error = StressError;

    fn now_ms(&mut self) -> f64 {
        self.now as f64 * 1e-6
    }

    fn now_ns(&mut self) -> u64 {
        self.calls.push("clock".into());
        self.now
    }
    fn clear_req_queue(&mut self) -> Result<(), Self::Error> {
        self.calls.push("flush".into());
        Ok(())
    }
    fn enqueue_frame(&mut self, request: u64) -> Result<(), Self::Error> {
        self.calls.push(format!("enqueue:{request}"));
        Ok(())
    }
    fn fences(&self, _: usize) -> (i32, i32) {
        (11, 12)
    }
    fn wait_for_sync(&mut self, fence: i32, timeout: u32) -> bool {
        self.calls.push(format!("wait:{fence}:{timeout}"));
        !self.fail_wait
    }
    fn destroy_sync(&mut self, slot: usize) -> Result<(), Self::Error> {
        self.calls.push(format!("destroy:{slot}"));
        Ok(())
    }
    fn stress(&mut self, _: StressPoint) -> Result<bool, Self::Error> {
        if self.fail_stress {
            Err(StressError)
        } else {
            Ok(false)
        }
    }
    fn sleep_ms(&mut self, _: u64) {}
}

fn event(request_id: u64, frame_id: u64, timestamp: u64) -> FrameEvent {
    FrameEvent {
        request_id,
        frame_id,
        timestamp,
        sof_status: 0,
    }
}

#[test]
fn stress_configuration_error_precedes_request_and_fence_mutation() {
    let mut camera = CameraRequests::new(0, 3, 10_000, false).unwrap();
    let mut io = Io {
        fail_stress: true,
        now: 123_000,
        ..Io::default()
    };
    let mut sync = FrameSync::new(1);
    let error = camera
        .handle_event(event(1, 1, 123_000), &mut sync, &mut io)
        .unwrap_err();
    assert!(matches!(
        error,
        openpilot_camerad::requests::RequestError::Io(StressError)
    ));
    assert_eq!(camera.request_id_last, 0);
    assert_eq!(camera.frame_id_raw_last, 0);
    assert_eq!(io.calls, ["clock"]);
}

#[test]
fn initial_queue_records_cutoff_after_flush_and_starts_at_request_one() {
    let mut camera = CameraRequests::new(0, 3, 10_000, false).unwrap();
    let mut io = Io {
        now: 123_000,
        ..Io::default()
    };
    camera.start(&mut io).unwrap();
    assert_eq!(
        io.calls,
        ["flush", "clock", "enqueue:1", "enqueue:2", "enqueue:3"]
    );
    assert_eq!(camera.last_requeue_ts, 123_000);
    let mut sync = FrameSync::new(1);
    io.calls.clear();
    camera
        .handle_event(event(1, 1, 122_999), &mut sync, &mut io)
        .unwrap();
    assert!(io.calls.is_empty());
}

#[test]
fn synchronization_frame_is_withheld_and_next_frame_is_reindexed() {
    let mut sync = FrameSync::new(2);
    assert!(!sync.observe(0, 10, 1_000_000, false));
    assert!(!sync.observe(1, 20, 1_200_000, false));
    assert!(sync.observe(0, 11, 51_000_000, false));
    assert_eq!(sync.frame_id(0, 11), 0);
    assert_eq!(sync.frame_id(1, 21), 0);
    let mut staggered = FrameSync::new(2);
    assert!(!staggered.observe(0, 1, 1_000_000, false));
    assert!(!staggered.observe(1, 1, 26_200_001, true));
    assert!(!staggered.is_synced());
    assert!(!staggered.observe(1, 41, 99_000_000, true));
    assert!(staggered.is_synced());
}

#[test]
fn failed_fence_flushes_before_clock_and_never_publishes_or_waits_for_bps() {
    let mut camera = CameraRequests::new(0, 3, 10_000, false).unwrap();
    let mut io = Io {
        fail_wait: true,
        now: 100_000,
        ..Io::default()
    };
    let mut sync = FrameSync::new(1);
    assert!(camera
        .handle_event(event(1, 1, 80_000), &mut sync, &mut io)
        .unwrap()
        .frame
        .is_none());
    assert_eq!(
        io.calls,
        [
            "clock",
            "wait:11:100",
            "flush",
            "clock",
            "enqueue:2",
            "enqueue:3",
            "enqueue:4"
        ]
    );
    io.calls.clear();
    camera
        .handle_event(event(2, 2, 99_999), &mut sync, &mut io)
        .unwrap();
    assert!(io.calls.is_empty());
}

#[test]
fn invalid_request_counter_uses_original_post_increment_threshold() {
    let mut camera = CameraRequests::new(0, 3, 10_000, false).unwrap();
    let mut io = Io {
        now: 100_000,
        ..Io::default()
    };
    let mut sync = FrameSync::new(1);
    for _ in 0..6 {
        camera
            .handle_event(event(0, 1, 100_000), &mut sync, &mut io)
            .unwrap();
    }
    assert!(!io.calls.iter().any(|call| call == "flush"));
    camera
        .handle_event(event(0, 1, 100_000), &mut sync, &mut io)
        .unwrap();
    assert_eq!(
        &io.calls[7..],
        ["flush", "clock", "enqueue:1", "enqueue:2", "enqueue:3"]
    );
}
