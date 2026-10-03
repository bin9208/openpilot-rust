use crate::startup::FrameSync;
use crate::timing::{CameraEventTiming, TimingSample};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct FrameEvent {
    pub request_id: u64,
    pub frame_id: u64,
    pub timestamp: u64,
    pub sof_status: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FrameMetadata {
    pub slot: usize,
    pub frame_id: u32,
    pub request_id: u32,
    pub timestamp_sof: u64,
    pub timestamp_eof: u64,
    pub processing_time: f32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum Disposition {
    Stale,
    StressSkipped,
    InvalidRequest,
    FrameGap,
    RequestGap,
    FenceFailure,
    Synchronizing,
    Ready,
}

#[derive(Debug)]
pub struct EventResult {
    pub frame: Option<FrameMetadata>,
    pub timing: Option<TimingSample>,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, Copy)]
pub enum StressPoint {
    SkipSof,
    SyncSleep,
    IfeWait,
    BpsWait,
}

#[derive(Debug)]
pub enum Diagnostic<'a> {
    Stale {
        camera: i32,
        event: FrameEvent,
    },
    Timing {
        camera: i32,
        event: FrameEvent,
        received: u64,
        sample: &'a TimingSample,
        previous_frame: u64,
        previous_request: u64,
        last_requeue: u64,
    },
    InvalidReset {
        camera: i32,
    },
    Gap {
        camera: i32,
        frame: bool,
        previous: u64,
        current: u64,
    },
    Requeue {
        camera: i32,
        from: u64,
    },
    SyncFailure {
        camera: i32,
        event: FrameEvent,
    },
    WaitFailure {
        camera: i32,
        point: StressPoint,
        elapsed_ms: f64,
    },
    Synchronization {
        camera: i32,
        event: FrameEvent,
        sync: &'a FrameSync,
    },
}

impl StressPoint {
    pub fn name(self) -> &'static str {
        match self {
            Self::SkipSof => "skipping SOF event",
            Self::SyncSleep => "sync sleep time",
            Self::IfeWait => "IFE sync",
            Self::BpsWait => "BPS sync",
        }
    }
}

pub trait RequestIo {
    type Error: std::error::Error + 'static;
    fn now_ns(&mut self) -> u64;
    fn now_ms(&mut self) -> f64;
    fn diagnostic(&mut self, _: Diagnostic<'_>) {}
    fn clear_req_queue(&mut self) -> Result<(), Self::Error>;
    fn enqueue_frame(&mut self, request: u64) -> Result<(), Self::Error>;
    fn fences(&self, slot: usize) -> (i32, i32);
    fn wait_for_sync(&mut self, fence: i32, timeout_ms: u32) -> bool;
    fn destroy_sync(&mut self, slot: usize) -> Result<(), Self::Error>;
    fn stress(&mut self, point: StressPoint) -> Result<bool, Self::Error>;
    fn sleep_ms(&mut self, millis: u64);
}

#[derive(Debug, thiserror::Error)]
#[error("camera buffer depth must be between 1 and 19, got {0}")]
pub struct InvalidDepth(pub usize);

#[derive(Debug, thiserror::Error)]
pub enum RequestError<E> {
    #[error("camera request I/O failed: {0}")]
    Io(E),
    #[error("missing IFE fence for request {0}")]
    MissingFence(u64),
}

#[derive(Debug, Serialize)]
pub struct CameraRequests {
    camera: i32,
    depth: usize,
    readout_ns: u64,
    staggered: bool,
    pub request_id_last: u64,
    pub last_requeue_ts: u64,
    pub frame_id_raw_last: u64,
    pub invalid_request_count: i32,
    pub skip_expected: bool,
    #[serde(skip)]
    timing: CameraEventTiming,
}

impl CameraRequests {
    pub fn new(
        camera: i32,
        depth: usize,
        readout_ns: u64,
        staggered: bool,
    ) -> Result<Self, InvalidDepth> {
        if !(1..20).contains(&depth) {
            return Err(InvalidDepth(depth));
        }
        Ok(Self {
            camera,
            depth,
            readout_ns,
            staggered,
            request_id_last: 0,
            last_requeue_ts: 0,
            frame_id_raw_last: 0,
            invalid_request_count: 0,
            skip_expected: true,
            timing: CameraEventTiming::default(),
        })
    }

    pub fn start<I: RequestIo>(&mut self, io: &mut I) -> Result<(), RequestError<I::Error>> {
        self.requeue(1, io)
    }

    pub fn handle_event<I: RequestIo>(
        &mut self,
        event: FrameEvent,
        sync: &mut FrameSync,
        io: &mut I,
    ) -> Result<EventResult, RequestError<I::Error>> {
        let mut result = EventResult {
            frame: None,
            timing: None,
            disposition: Disposition::Stale,
        };
        if event.timestamp < self.last_requeue_ts {
            io.diagnostic(Diagnostic::Stale {
                camera: self.camera,
                event,
            });
            return Ok(result);
        }
        let received = io.now_ns();
        let timing = self.timing.observe(event.timestamp, received);
        if timing.report {
            io.diagnostic(Diagnostic::Timing {
                camera: self.camera,
                event,
                received,
                sample: &timing,
                previous_frame: self.frame_id_raw_last,
                previous_request: self.request_id_last,
                last_requeue: self.last_requeue_ts,
            });
        }
        result.timing = Some(timing);
        if io.stress(StressPoint::SkipSof).map_err(RequestError::Io)? {
            result.disposition = Disposition::StressSkipped;
            return Ok(result);
        }
        if let Some(disposition) = self.validate(event, io)? {
            result.disposition = disposition;
            return Ok(result);
        }
        if event.request_id == self.request_id_last.wrapping_add(1) {
            self.skip_expected = false;
        }
        self.frame_id_raw_last = event.frame_id;
        self.request_id_last = event.request_id;
        if !self.wait_for_frame(event.request_id, io)? {
            io.diagnostic(Diagnostic::SyncFailure {
                camera: self.camera,
                event,
            });
            self.requeue(event.request_id.wrapping_add(1), io)?;
            result.disposition = Disposition::FenceFailure;
            return Ok(result);
        }
        let slot = (event.request_id % self.depth as u64) as usize;
        result.disposition = Disposition::Synchronizing;
        let synchronized =
            sync.observe(self.camera, event.frame_id, event.timestamp, self.staggered);
        if !synchronized {
            io.diagnostic(Diagnostic::Synchronization {
                camera: self.camera,
                event,
                sync,
            });
        }
        if synchronized {
            let eof = event.timestamp.wrapping_add(self.readout_ns);
            result.frame = Some(FrameMetadata {
                slot,
                frame_id: sync.frame_id(self.camera, event.frame_id),
                request_id: event.request_id as u32,
                timestamp_sof: event.timestamp,
                timestamp_eof: eof,
                processing_time: (io.now_ns().wrapping_sub(eof) as f64 * 1e-9) as f32,
            });
            result.disposition = Disposition::Ready;
        }
        io.destroy_sync(slot).map_err(RequestError::Io)?;
        io.enqueue_frame(event.request_id.wrapping_add(self.depth as u64))
            .map_err(RequestError::Io)?;
        Ok(result)
    }

    fn validate<I: RequestIo>(
        &mut self,
        event: FrameEvent,
        io: &mut I,
    ) -> Result<Option<Disposition>, RequestError<I::Error>> {
        if event.request_id == 0 {
            let previous = self.invalid_request_count;
            self.invalid_request_count += 1;
            if previous > self.depth as i32 + 2 {
                io.diagnostic(Diagnostic::InvalidReset {
                    camera: self.camera,
                });
                self.requeue(self.request_id_last.wrapping_add(1), io)?;
                self.invalid_request_count = 0;
            }
            return Ok(Some(Disposition::InvalidRequest));
        }
        self.invalid_request_count = 0;
        let gap = if !self.skip_expected && event.frame_id != self.frame_id_raw_last.wrapping_add(1)
        {
            io.diagnostic(Diagnostic::Gap {
                camera: self.camera,
                frame: true,
                previous: self.frame_id_raw_last,
                current: event.frame_id,
            });
            Some(Disposition::FrameGap)
        } else if !self.skip_expected && event.request_id != self.request_id_last.wrapping_add(1) {
            io.diagnostic(Diagnostic::Gap {
                camera: self.camera,
                frame: false,
                previous: self.request_id_last,
                current: event.request_id,
            });
            Some(Disposition::RequestGap)
        } else {
            None
        };
        if gap.is_some() {
            self.requeue(event.request_id.wrapping_add(1), io)?;
        }
        Ok(gap)
    }

    fn requeue<I: RequestIo>(
        &mut self,
        from: u64,
        io: &mut I,
    ) -> Result<(), RequestError<I::Error>> {
        io.diagnostic(Diagnostic::Requeue {
            camera: self.camera,
            from,
        });
        io.clear_req_queue().map_err(RequestError::Io)?;
        self.last_requeue_ts = io.now_ns();
        for request in from..from.wrapping_add(self.depth as u64) {
            io.enqueue_frame(request).map_err(RequestError::Io)?;
        }
        self.skip_expected = true;
        Ok(())
    }

    fn wait_for_frame<I: RequestIo>(
        &self,
        request: u64,
        io: &mut I,
    ) -> Result<bool, RequestError<I::Error>> {
        let (ife, bps) = io.fences((request % self.depth as u64) as usize);
        if ife == 0 {
            return Err(RequestError::MissingFence(request));
        }
        if io
            .stress(StressPoint::SyncSleep)
            .map_err(RequestError::Io)?
        {
            io.sleep_ms(350);
            return Ok(false);
        }
        let mut success = self.wait_for_sync(ife, 100, StressPoint::IfeWait, io)?;
        if success && bps != 0 {
            success = self.wait_for_sync(bps, 50, StressPoint::BpsWait, io)?;
        }
        Ok(success)
    }

    fn wait_for_sync<I: RequestIo>(
        &self,
        fence: i32,
        timeout: u32,
        point: StressPoint,
        io: &mut I,
    ) -> Result<bool, RequestError<I::Error>> {
        let start = io.now_ms();
        let timeout = if io.stress(point).map_err(RequestError::Io)? {
            1
        } else {
            timeout
        };
        let success = io.wait_for_sync(fence, timeout);
        let end = io.now_ms();
        if !success {
            io.diagnostic(Diagnostic::WaitFailure {
                camera: self.camera,
                point,
                elapsed_ms: end - start,
            });
        }
        Ok(success)
    }
}
