use super::Frame;
use crate::scene::math::clamp;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Chunk {
    pub index: usize,
    pub path: PathBuf,
    pub frames: Vec<Arc<Frame>>,
    pub start_t: f64,
    pub end_t: f64,
}
#[derive(Default)]
pub struct Timeline {
    chunks: VecDeque<Chunk>,
    frames: Vec<Arc<Frame>>,
    times: Vec<f64>,
    duration: f64,
}
impl Timeline {
    pub fn append(&mut self, index: usize, path: PathBuf, frames: Vec<Frame>) {
        let (Some(first), Some(last)) = (frames.first(), frames.last()) else {
            return;
        };
        let (start_t, end_t) = (first.t, last.t);
        self.chunks.push_back(Chunk {
            index,
            path,
            frames: frames.into_iter().map(Arc::new).collect(),
            start_t,
            end_t,
        });
        self.rebuild();
    }
    pub fn trim(&mut self, time: f64) {
        let mut removed = false;
        while self.chunks.len() > 2
            && self
                .chunks
                .front()
                .is_some_and(|chunk| chunk.end_t < time - 1.0)
        {
            self.chunks.pop_front();
            removed = true;
        }
        if removed {
            self.rebuild();
        }
    }
    fn rebuild(&mut self) {
        self.frames = self
            .chunks
            .iter()
            .flat_map(|chunk| chunk.frames.iter().cloned())
            .collect();
        self.frames
            .sort_by(|left, right| left.t.total_cmp(&right.t));
        self.times = self.frames.iter().map(|frame| frame.t).collect();
        if let Some(frame) = self.frames.last() {
            self.duration = self.duration.max(frame.t);
        }
    }
    #[must_use]
    pub fn chunks(&self) -> &VecDeque<Chunk> {
        &self.chunks
    }
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    #[must_use]
    pub fn duration(&self) -> f64 {
        self.duration
    }
    #[must_use]
    pub fn first_time(&self) -> Option<f64> {
        self.times.first().copied()
    }
    #[must_use]
    pub fn playback_frame(&self, time: f64) -> Option<Frame> {
        if self.duration <= 0.0 {
            return self.frames.first().map(|frame| frame.as_ref().clone());
        }
        self.sample(clamp(time, 0.0, self.duration))
    }
    #[must_use]
    pub fn status_frame(&self, time: f64) -> Option<Frame> {
        if time <= *self.times.first()? {
            return self.frames.first().map(|frame| frame.as_ref().clone());
        }
        self.sample(time)
    }
    pub(super) fn sample(&self, time: f64) -> Option<Frame> {
        let right = self.times.partition_point(|value| *value <= time);
        if right == 0 {
            return self.frames.first().map(|frame| frame.as_ref().clone());
        }
        if right >= self.frames.len() {
            return self.frames.last().map(|frame| frame.as_ref().clone());
        }
        let (left, right) = (&self.frames[right - 1], &self.frames[right]);
        let amount = clamp((time - left.t) / (right.t - left.t).max(0.001), 0.0, 1.0);
        Some(left.blend(right, amount))
    }
    #[must_use]
    pub fn log_path(&self, time: f64) -> Option<&Path> {
        let chunk = self
            .chunks
            .iter()
            .find(|chunk| chunk.start_t - 0.5 <= time && time <= chunk.end_t + 0.5)
            .or_else(|| {
                self.chunks.iter().min_by(|left, right| {
                    let distance = |chunk: &Chunk| {
                        (time - chunk.start_t).abs().min((time - chunk.end_t).abs())
                    };
                    distance(left).total_cmp(&distance(right))
                })
            })?;
        Some(&chunk.path)
    }
    #[must_use]
    pub fn log_offset(&self, time: f64) -> f64 {
        let Some(path) = self.log_path(time) else {
            return 0.0;
        };
        self.chunks
            .iter()
            .find(|chunk| chunk.path == path)
            .map_or(0.0, |chunk| {
                clamp(
                    time - chunk.start_t,
                    0.0,
                    (chunk.end_t - chunk.start_t).max(0.0),
                )
            })
    }
}
