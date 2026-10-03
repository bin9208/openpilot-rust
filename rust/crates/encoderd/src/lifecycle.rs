use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Lifecycle {
    pub segment: i32,
    pub prewarm_pending: bool,
    pub lagging: bool,
    pub on_demand: bool,
    pub segment_length: i32,
}
impl Lifecycle {
    pub fn new(on_demand: bool, segment_length: i32) -> Self {
        Self {
            segment: 0,
            prewarm_pending: on_demand,
            lagging: false,
            on_demand,
            segment_length,
        }
    }
    pub fn matching_frame(&mut self, input: Frame) -> FrameDecision {
        if input.buffer_frame_id != u64::from(input.frame_id) {
            let log_lag = !self.lagging;
            self.lagging = true;
            return FrameDecision {
                log_lag,
                encode: false,
                idle: None,
                rotate: false,
                thumbnail: false,
            };
        }
        self.lagging = false;
        if !input.synced || input.exit {
            return FrameDecision {
                log_lag: false,
                encode: false,
                idle: None,
                rotate: false,
                thumbnail: false,
            };
        }
        let active = !self.on_demand || input.session_active;
        let idle = !active && !self.prewarm_pending;
        if idle {
            return FrameDecision {
                log_lag: false,
                encode: false,
                idle: Some(true),
                rotate: false,
                thumbnail: false,
            };
        }
        let per_segment = self.segment_length.wrapping_mul(20);
        let threshold = self.segment.wrapping_add(1).wrapping_mul(per_segment);
        let threshold =
            u32::from_ne_bytes(threshold.to_ne_bytes()).wrapping_add(input.start_frame_id);
        let rotate = !self.on_demand && self.segment >= 0 && input.frame_id >= threshold;
        FrameDecision {
            log_lag: false,
            encode: true,
            idle: Some(false),
            rotate,
            thumbnail: input.frame_id % 1200 == 100,
        }
    }
    pub fn rotated(&mut self) {
        self.segment = self.segment.wrapping_add(1);
    }
    pub fn encoded(&mut self) {
        self.prewarm_pending = false;
    }
}
#[derive(Clone, Copy)]
pub struct Frame {
    pub buffer_frame_id: u64,
    pub frame_id: u32,
    pub synced: bool,
    pub exit: bool,
    pub session_active: bool,
    pub start_frame_id: u32,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FrameDecision {
    pub log_lag: bool,
    pub encode: bool,
    pub idle: Option<bool>,
    pub rotate: bool,
    pub thumbnail: bool,
}
