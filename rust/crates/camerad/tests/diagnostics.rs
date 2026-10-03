use openpilot_camerad::{
    requests::{CameraRequests, Diagnostic, FrameEvent, RequestIo, StressPoint},
    startup::FrameSync,
};
use std::convert::Infallible;

#[derive(Default)]
struct Io {
    calls: Vec<String>,
    fail: bool,
}

impl RequestIo for Io {
    type Error = Infallible;
    fn now_ns(&mut self) -> u64 {
        self.calls.push("ns".into());
        200_000_000
    }
    fn now_ms(&mut self) -> f64 {
        self.calls.push("ms".into());
        200.0
    }
    fn diagnostic(&mut self, value: Diagnostic<'_>) {
        self.calls.push(match value {
            Diagnostic::Timing {
                previous_frame,
                previous_request,
                ..
            } => {
                format!("timing:{previous_frame}:{previous_request}")
            }
            Diagnostic::WaitFailure {
                point, elapsed_ms, ..
            } => {
                format!("failure:{}:{elapsed_ms}", point.name())
            }
            Diagnostic::SyncFailure { .. } => "sync_failure".into(),
            Diagnostic::Requeue { .. } => "requeue".into(),
            Diagnostic::Synchronization { sync, .. } => {
                format!("synced:{}", sync.transition().synchronized)
            }
            value => format!("{value:?}"),
        });
    }
    fn clear_req_queue(&mut self) -> Result<(), Self::Error> {
        self.calls.push("flush".into());
        Ok(())
    }
    fn enqueue_frame(&mut self, request: u64) -> Result<(), Self::Error> {
        self.calls.push(format!("queue:{request}"));
        Ok(())
    }
    fn fences(&self, _: usize) -> (i32, i32) {
        (11, 12)
    }
    fn wait_for_sync(&mut self, fence: i32, _: u32) -> bool {
        self.calls.push(format!("wait:{fence}"));
        !self.fail
    }
    fn destroy_sync(&mut self, _: usize) -> Result<(), Self::Error> {
        self.calls.push("destroy".into());
        Ok(())
    }
    fn stress(&mut self, point: StressPoint) -> Result<bool, Self::Error> {
        self.calls.push(format!("stress:{}", point.name()));
        Ok(false)
    }
    fn sleep_ms(&mut self, _: u64) {}
}

#[test]
fn wait_clock_surrounds_stress_and_ioctl_and_failure_logs_precede_requeue() {
    let mut io = Io {
        fail: true,
        ..Io::default()
    };
    let mut requests = CameraRequests::new(0, 1, 10, false).unwrap();
    requests
        .handle_event(
            FrameEvent {
                request_id: 1,
                frame_id: 1,
                timestamp: 100_000_000,
                sof_status: 7,
            },
            &mut FrameSync::new(1),
            &mut io,
        )
        .unwrap();
    assert_eq!(
        io.calls,
        [
            "ns",
            "timing:0:0",
            "stress:skipping SOF event",
            "stress:sync sleep time",
            "ms",
            "stress:IFE sync",
            "wait:11",
            "ms",
            "failure:IFE sync:0",
            "sync_failure",
            "requeue",
            "flush",
            "ns",
            "queue:2"
        ]
    );
}

#[test]
fn successful_bps_also_reads_both_clocks_before_startup_synchronization() {
    let mut io = Io::default();
    let mut requests = CameraRequests::new(0, 1, 10, false).unwrap();
    requests
        .handle_event(
            FrameEvent {
                request_id: 1,
                frame_id: 1,
                timestamp: 200_000_000,
                sof_status: 0,
            },
            &mut FrameSync::new(1),
            &mut io,
        )
        .unwrap();
    assert_eq!(
        io.calls,
        [
            "ns",
            "stress:skipping SOF event",
            "stress:sync sleep time",
            "ms",
            "stress:IFE sync",
            "wait:11",
            "ms",
            "ms",
            "stress:BPS sync",
            "wait:12",
            "ms",
            "synced:true",
            "destroy",
            "queue:2"
        ]
    );
}
