use openpilot_camerad::{
    requests::{CameraRequests, FrameEvent, FrameMetadata, RequestIo, StressPoint},
    startup::FrameSync,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    error::Error,
    io::{self, BufRead, Write},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Input {
    Reset {
        depth: usize,
        enabled: u32,
        bps: u32,
        ar: u32,
        staggered: u32,
        readout: u64,
    },
    Step {
        camera: usize,
        request: u64,
        frame: u64,
        timestamp: u64,
        status: u32,
        first_now: u64,
        later_now: u64,
        failures: u32,
        stress: u32,
    },
}

struct Io {
    depth: usize,
    bps: bool,
    fences: [(i32, i32); 20],
    first_now: u64,
    later_now: u64,
    clock_calls: u32,
    failures: u32,
    stress: u32,
    calls: Vec<Value>,
}

impl RequestIo for Io {
    type Error = Infallible;
    fn now_ms(&mut self) -> f64 {
        self.later_now as f64 * 1e-6
    }
    fn now_ns(&mut self) -> u64 {
        self.calls.push(json!("clock"));
        self.clock_calls += 1;
        if self.clock_calls == 1 {
            self.first_now
        } else {
            self.later_now
        }
    }
    fn clear_req_queue(&mut self) -> Result<(), Self::Error> {
        self.calls.push(json!("flush"));
        self.fences.fill((0, 0));
        Ok(())
    }
    fn enqueue_frame(&mut self, request: u64) -> Result<(), Self::Error> {
        self.calls.push(json!(["enqueue", request]));
        let slot = (request % self.depth as u64) as usize;
        self.fences[slot] = (
            100 + slot as i32,
            if self.bps { 200 + slot as i32 } else { 0 },
        );
        Ok(())
    }
    fn fences(&self, slot: usize) -> (i32, i32) {
        self.fences[slot]
    }
    fn wait_for_sync(&mut self, fence: i32, timeout_ms: u32) -> bool {
        self.calls.push(json!(["wait", fence, timeout_ms]));
        self.failures & if fence >= 200 { 2 } else { 1 } == 0
    }
    fn destroy_sync(&mut self, slot: usize) -> Result<(), Self::Error> {
        self.calls.push(json!(["destroy", slot]));
        self.fences[slot] = (0, 0);
        Ok(())
    }
    fn stress(&mut self, point: StressPoint) -> Result<bool, Self::Error> {
        self.calls.push(json!(["stress", point.name()]));
        Ok(self.stress
            & match point {
                StressPoint::SkipSof => 1,
                StressPoint::SyncSleep => 2,
                StressPoint::IfeWait => 4,
                StressPoint::BpsWait => 8,
            }
            != 0)
    }
    fn sleep_ms(&mut self, millis: u64) {
        self.calls.push(json!(["sleep", millis]));
    }
}

struct Camera {
    requests: CameraRequests,
    io: Io,
    frame: Option<FrameMetadata>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut cameras: Vec<Camera> = Vec::new();
    let mut sync = FrameSync::new(0);
    for line in io::stdin().lock().lines() {
        let value = match serde_json::from_str::<Input>(&line?)? {
            Input::Reset {
                depth,
                enabled,
                bps,
                ar,
                staggered,
                readout,
            } => {
                cameras.clear();
                sync = FrameSync::new((enabled & 7).count_ones() as usize);
                for camera in 0..3 {
                    let bps = bps & (1 << camera) != 0;
                    let mut fences = [(0, 0); 20];
                    for (slot, fence) in fences.iter_mut().take(depth).enumerate() {
                        *fence = (100 + slot as i32, if bps { 200 + slot as i32 } else { 0 });
                    }
                    cameras.push(Camera {
                        requests: CameraRequests::new(
                            camera,
                            depth,
                            readout,
                            staggered & (1 << camera) != 0 && ar & (1 << camera) == 0,
                        )?,
                        io: Io {
                            depth,
                            bps,
                            fences,
                            first_now: 0,
                            later_now: 0,
                            clock_calls: 0,
                            failures: 0,
                            stress: 0,
                            calls: Vec::new(),
                        },
                        frame: None,
                    });
                }
                json!({})
            }
            Input::Step {
                camera,
                request,
                frame,
                timestamp,
                status,
                first_now,
                later_now,
                failures,
                stress,
            } => {
                let camera = cameras
                    .get_mut(camera)
                    .ok_or("camera index is out of range")?;
                let io = &mut camera.io;
                io.first_now = first_now;
                io.later_now = later_now;
                io.clock_calls = 0;
                io.failures = failures;
                io.stress = stress;
                io.calls.clear();
                let result = camera.requests.handle_event(
                    FrameEvent {
                        request_id: request,
                        frame_id: frame,
                        timestamp,
                        sof_status: status,
                    },
                    &mut sync,
                    io,
                )?;
                let accepted = result.frame.is_some();
                if accepted {
                    camera.frame = result.frame;
                }
                let state = &camera.requests;
                json!({
                    "accepted": accepted,
                    "request_id_last": state.request_id_last,
                    "frame_id_raw_last": state.frame_id_raw_last,
                    "last_requeue_ts": state.last_requeue_ts,
                    "invalid_request_count": state.invalid_request_count,
                    "skip_expected": state.skip_expected,
                    "synced": sync.is_synced(),
                    "sync": sync.cameras().iter().map(|(id, data)| json!([id, data.timestamp, data.frame_id_offset, data.staggered])).collect::<Vec<_>>(),
                    "frame": camera.frame.map(|f| json!([f.slot, f.frame_id, f.request_id, f.timestamp_sof, f.timestamp_eof, f.processing_time.to_bits()])),
                    "fences": io.fences,
                    "calls": io.calls,
                })
            }
        };
        serde_json::to_writer(&mut output, &value)?;
        writeln!(output)?;
    }
    output.flush()?;
    Ok(())
}
