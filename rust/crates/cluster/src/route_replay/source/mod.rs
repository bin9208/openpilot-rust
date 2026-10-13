mod loading;
mod view;
use super::{
    paths, preload,
    timeline::Timeline,
    video::{Reader, Segment, VideoConfig},
    Error,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct SourceConfig {
    pub preload: preload::Config,
    pub video: VideoConfig,
    pub yaw_gain: f64,
    pub lateral_offset: f64,
    pub front_radar_only: Option<bool>,
}
impl SourceConfig {
    /// # Errors
    /// Returns video-provider discovery errors.
    pub fn for_preload(preload: preload::Config) -> Result<Self, Error> {
        let video = VideoConfig::discover(None, preload.cwd.clone())?;
        Ok(Self {
            preload,
            video,
            yaw_gain: 0.0,
            lateral_offset: 0.0,
            front_radar_only: None,
        })
    }
}
pub struct Source {
    files: Vec<PathBuf>,
    config: SourceConfig,
    timeline: Timeline,
    worker: preload::Worker,
    reader: Reader,
    streams: [Vec<Segment>; 2],
    stream: usize,
    next: usize,
    loaded: usize,
    end: bool,
    active: Option<preload::ParseRequest>,
    generation: u64,
}
impl Source {
    /// # Errors
    /// Returns empty-route, preload or initial log parsing errors.
    pub fn new(files: Vec<PathBuf>, config: SourceConfig) -> Result<Self, Error> {
        if files.is_empty() {
            return Err(Error::Invalid("route contains no log files".into()));
        }
        let reader = Reader::new(Vec::new(), "road", config.video.clone());
        let worker = preload::Worker::configured(config.preload.clone(), config.front_radar_only)?;
        let mut source = Self {
            files,
            config,
            timeline: Timeline::default(),
            worker,
            reader,
            streams: [Vec::new(), Vec::new()],
            stream: 0,
            next: 0,
            loaded: 0,
            end: false,
            active: None,
            generation: 0,
        };
        source.ensure_loaded(0.0)?;
        if source.timeline.is_empty() {
            return Err(Error::Invalid("route contains no carState frames".into()));
        }
        Ok(source)
    }
    /// # Errors
    /// Returns route discovery, preload or initial log parsing errors.
    pub fn load(
        path: &Path,
        kind: &str,
        start: Option<i64>,
        maximum: Option<i64>,
        config: SourceConfig,
    ) -> Result<Self, Error> {
        let files = paths::discover_route_logs(path, kind, start, maximum)?;
        if files.is_empty() {
            return Err(Error::Invalid(format!(
                "no {kind}.zst files found under {}",
                path.display()
            )));
        }
        Self::new(files, config)
    }
    #[must_use]
    pub fn duration(&self) -> f64 {
        self.timeline.duration()
    }
    #[must_use]
    pub fn loaded_file_count(&self) -> usize {
        self.loaded
    }
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.timeline
            .chunks()
            .iter()
            .map(|chunk| chunk.frames.len())
            .sum()
    }
    #[must_use]
    pub fn first_file(&self) -> &Path {
        &self.files[0]
    }
    pub fn set_corner_lateral_offset(&mut self, offset: f64) {
        self.config.lateral_offset = offset;
    }
    #[must_use]
    pub fn log_path_at(&self, time: f64) -> &Path {
        self.timeline.log_path(time).unwrap_or(&self.files[0])
    }
    #[must_use]
    pub fn log_folder_at(&self, time: f64) -> PathBuf {
        self.log_path_at(time)
            .parent()
            .unwrap_or(Path::new("."))
            .into()
    }
    #[must_use]
    pub fn log_offset_at(&self, time: f64) -> f64 {
        self.timeline.log_offset(time)
    }
    /// # Errors
    /// Returns preload errors while advancing the route.
    pub fn is_finished(&mut self, time: f64, repeat: bool) -> Result<bool, Error> {
        if !repeat {
            self.ensure_loaded(time)?;
        }
        Ok(!repeat && time > self.duration())
    }
    /// # Errors
    /// Returns generation, child shutdown or video cleanup errors.
    pub fn close(&mut self) -> Result<(), Error> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("route preload generation overflow".into()))?;
        self.active = None;
        let preload = self.worker.close();
        let video = self.reader.close();
        preload.and(video)
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("route replay close: {error}");
        }
    }
}
