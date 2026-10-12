//! Source route camera selection and an owned, lazy `FFmpeg` frame reader.
mod config;
mod process;
mod reader;
mod repr;
pub use config::VideoConfig;
pub use reader::Reader;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Segment {
    pub index: Option<i64>,
    pub path: PathBuf,
    pub start_t: f64,
    pub end_t: f64,
}
#[derive(Clone, Debug)]
pub struct Frame {
    pub rgba: Arc<[u8]>,
    pub width: i32,
    pub height: i32,
    pub frame_id: String,
}
/// Selects the source candidate including the half-second end grace and next gap.
#[must_use]
pub fn segment_at(segments: &[Segment], time: f64) -> Option<&Segment> {
    if segments.is_empty() {
        return None;
    }
    // bisect_right uses !(time < start), including Python's NaN comparison order.
    let insertion = segments.partition_point(|segment| {
        !matches!(
            time.partial_cmp(&segment.start_t),
            Some(std::cmp::Ordering::Less)
        )
    });
    let Some(index) = insertion.checked_sub(1) else {
        return segments.first();
    };
    let segment = &segments[index];
    if time <= segment.end_t + 0.5 {
        Some(segment)
    } else {
        segments.get(index + 1).or(Some(segment))
    }
}
#[must_use]
pub fn path_for_stream(folder: &Path, stream: &str) -> Option<PathBuf> {
    let filenames: &[&str] = if stream == "wide" {
        &["ecamera.hevc", "ecamera.ts"]
    } else {
        &["qcamera.ts", "fcamera.hevc", "fcamera.ts"]
    };
    filenames
        .iter()
        .map(|name| folder.join(name))
        .find(|path| path.is_file())
}
