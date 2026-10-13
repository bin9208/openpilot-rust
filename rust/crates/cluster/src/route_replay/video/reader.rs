use super::{process::Owned, repr, segment_at, Frame, Segment, VideoConfig};
use crate::route_replay::Error;
use num_traits::ToPrimitive;
use std::{
    io::{self, Read},
    process::{Command, Stdio},
    sync::Arc,
};
const WIDTH: i32 = 388;
const HEIGHT: i32 = 244;
const BYTES: usize = 388 * 244 * 4;
pub struct Reader {
    segments: Vec<Segment>,
    stream: String,
    config: VideoConfig,
    process: Option<Owned>,
    segment_key: Option<Segment>,
    frame_index: i64,
    last_frame: Option<Frame>,
    status: String,
}
impl Reader {
    #[must_use]
    pub fn new(segments: Vec<Segment>, stream: &str, config: VideoConfig) -> Self {
        Self {
            segments,
            stream: stream.into(),
            config,
            process: None,
            segment_key: None,
            frame_index: -1,
            last_frame: None,
            status: format!("{stream} camera waiting"),
        }
    }
    pub fn set_segments(&mut self, segments: Vec<Segment>) {
        self.segments = segments;
    }
    #[must_use]
    pub fn status_text(&self) -> String {
        if self.status.is_empty() {
            format!("{} camera unavailable", self.stream)
        } else {
            self.status.clone()
        }
    }
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.child.id())
    }
    /// # Errors
    /// Returns a malformed frame clock, pipe I/O or owned-child cleanup failure.
    pub fn frame_at(&mut self, time: f64) -> Result<Option<Frame>, Error> {
        let Some(segment) = segment_at(&self.segments, time).cloned() else {
            self.close()?;
            self.status = format!("{} camera missing", self.stream);
            return Ok(None);
        };
        if !segment.path.exists() {
            self.close()?;
            self.status = format!("{} camera file missing", self.stream);
            return Ok(None);
        }
        if self.config.ffmpeg.is_none() {
            self.status = format!("{} camera ffmpeg missing", self.stream);
            return Ok(None);
        }
        let local = (time - segment.start_t).clamp(0.0, (segment.end_t - segment.start_t).max(0.0));
        let target = (local * 10.0)
            .trunc()
            .to_i64()
            .ok_or_else(|| Error::Invalid("route video frame clock is not representable".into()))?
            .max(0);
        let restart = self.process.is_none()
            || self.segment_key.as_ref() != Some(&segment)
            || target < self.frame_index
            || target.saturating_sub(self.frame_index) > 45;
        if restart && !self.open(&segment, target)? {
            return Ok(self.last_frame.clone());
        }
        while self.frame_index < target {
            if !self.read_next()? {
                return Ok(self.last_frame.clone());
            }
        }
        self.status.clear();
        Ok(self.last_frame.clone())
    }
    fn open(&mut self, segment: &Segment, start_frame: i64) -> Result<bool, Error> {
        self.close()?;
        let Some(ffmpeg) = &self.config.ffmpeg else {
            self.status = format!("{} camera ffmpeg missing", self.stream);
            return Ok(false);
        };
        let seek = start_frame
            .to_f64()
            .ok_or_else(|| Error::Invalid("route video seek is not representable".into()))?
            / 10.0;
        let result = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-threads", "1", "-i"])
            .arg(&segment.path)
            .args([
                "-ss",
                &format!("{seek:.3}"),
                "-an",
                "-sn",
                "-vf",
                "fps=10,scale=388:244",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .current_dir(&self.config.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let child = match result {
            Ok(child) => child,
            Err(error) => {
                let display = error.to_string();
                let description = display
                    .rsplit_once(" (os error ")
                    .map_or(display.as_str(), |(message, _)| message);
                self.status = format!(
                    "{} camera ffmpeg failed: [Errno {}] {}: {}",
                    self.stream,
                    error.raw_os_error().unwrap_or(0),
                    description,
                    repr::path_repr(ffmpeg)?
                );
                return Ok(false);
            }
        };
        self.process = Some(Owned::new(child));
        self.segment_key = Some(segment.clone());
        self.frame_index = start_frame - 1;
        self.last_frame = None;
        self.status = format!("{} camera starting", self.stream);
        Ok(true)
    }
    fn read_next(&mut self) -> Result<bool, Error> {
        let Some(stdout) = self.process.as_mut().and_then(|p| p.child.stdout.as_mut()) else {
            self.status = format!("{} camera unavailable", self.stream);
            return Ok(false);
        };
        let mut bytes = vec![0; BYTES];
        let mut offset = 0;
        while offset < BYTES {
            match stdout.read(&mut bytes[offset..]) {
                Ok(0) => {
                    self.status = format!("{} camera ended", self.stream);
                    self.close()?;
                    return Ok(false);
                }
                Ok(count) => offset += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.frame_index += 1;
        let key = self
            .segment_key
            .as_ref()
            .ok_or_else(|| Error::Invalid("route video frame has no segment key".into()))?;
        self.last_frame = Some(Frame {
            rgba: Arc::from(bytes),
            width: WIDTH,
            height: HEIGHT,
            frame_id: format!("{}:{}", repr::key_repr(key)?, self.frame_index),
        });
        Ok(true)
    }
    /// # Errors
    /// Returns errors terminating and reaping this reader's owned `FFmpeg` child.
    pub fn close(&mut self) -> Result<(), Error> {
        self.segment_key = None;
        self.frame_index = -1;
        if let Some(mut process) = self.process.take() {
            process.close()?;
        }
        Ok(())
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("route video cleanup: {error}");
        }
    }
}
