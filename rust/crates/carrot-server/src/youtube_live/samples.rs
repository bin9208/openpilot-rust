use super::{input::Frame, profiles::Profile};
use num_traits::ToPrimitive;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Warmup {
    pub quality: u8,
    pub started: f64,
    pub frames: u64,
    pub bytes: u64,
    pub kbps: f64,
    last_id: Option<i128>,
}
impl Warmup {
    pub fn observe(&mut self, frame: &Frame, profile: Profile, now: f64) -> bool {
        if frame.width != u32::from(profile.width) || frame.height != u32::from(profile.height) {
            *self = Self::default();
            return false;
        }
        if self.started == 0.0 || self.quality != profile.quality {
            *self = Self {
                quality: profile.quality,
                started: now,
                ..Self::default()
            };
        }
        if frame
            .id
            .zip(self.last_id)
            .is_some_and(|(next, last)| next <= last)
        {
            self.started = now;
            self.frames = 0;
            self.bytes = 0;
            self.kbps = 0.0;
        }
        self.last_id = frame.id;
        self.frames = self.frames.saturating_add(1);
        self.bytes = self.bytes.saturating_add(
            u64::try_from(frame.header.len().saturating_add(frame.data.len())).unwrap_or(u64::MAX),
        );
        let elapsed = (now - self.started).max(0.001);
        self.kbps = self.bytes.to_f64().unwrap_or(0.0) * 8.0 / 1000.0 / elapsed;
        elapsed >= 6.0 && self.frames >= 45
    }
}
#[derive(Default)]
pub(super) struct Samples {
    source: VecDeque<(f64, usize, bool)>,
    sent: VecDeque<(f64, i128)>,
    pub bytes: u64,
    pub frames: u64,
    pub keyframes: u64,
    pub starved_since: f64,
}
impl Samples {
    pub fn record(&mut self, now: f64, bytes: usize, keyframe: bool) {
        if self.source.len() == 800 {
            self.source.pop_front();
        }
        self.source.push_back((now, bytes, keyframe));
        self.bytes = self
            .bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        self.frames = self.frames.saturating_add(1);
        self.keyframes = self.keyframes.saturating_add(u64::from(keyframe));
    }
    pub fn recent(&mut self, now: f64) -> (u64, f64) {
        while self
            .source
            .front()
            .is_some_and(|(time, _, _)| *time < now - 10.0)
        {
            self.source.pop_front();
        }
        let Some((first, _, _)) = self.source.front() else {
            return (0, 0.0);
        };
        let elapsed = (now - first).clamp(0.001, 10.0);
        let bytes = self
            .source
            .iter()
            .map(|(_, bytes, _)| bytes.to_f64().unwrap_or(0.0))
            .sum::<f64>();
        let kbps = (bytes * 8.0 / 1000.0 / elapsed).to_u64().unwrap_or(0);
        let fps = super::clock::round(self.source.len().to_f64().unwrap_or(0.0) / elapsed, 1);
        (kbps, fps)
    }
    pub fn upload_recent(&mut self, now: f64, bytes: i128) -> Option<u64> {
        while self
            .sent
            .front()
            .is_some_and(|(time, _)| *time < now - 10.0)
        {
            self.sent.pop_front();
        }
        if self.sent.len() < 2 {
            return None;
        }
        let (first, first_bytes) = *self.sent.front()?;
        let elapsed = now - first;
        if elapsed < 8.0 {
            return None;
        }
        (bytes.saturating_sub(first_bytes).to_f64()? * 8.0 / 1000.0 / elapsed.max(0.001)).to_u64()
    }
    pub fn starved(&mut self, stream: (f64, i128), target: Profile, now: f64) -> Option<String> {
        if self.sent.len() == 800 {
            self.sent.pop_front();
        }
        self.sent.push_back((now, stream.1));
        if stream.0 == 0.0 || now - stream.0 < 20.0 {
            self.starved_since = 0.0;
            return None;
        }
        let required = (f64::from(target.video_kbps) * 0.55).to_u64().unwrap_or(0);
        let kbps = self.upload_recent(now, stream.1)?;
        if required == 0 {
            return None;
        }
        if kbps >= required {
            self.starved_since = 0.0;
            return None;
        }
        if self.starved_since == 0.0 {
            self.starved_since = now;
            return None;
        }
        if now - self.starved_since < 15.0 {
            return None;
        }
        self.starved_since = 0.0;
        Some(format!(
            "upload starved: {kbps} kbps below required {required} kbps"
        ))
    }
}
