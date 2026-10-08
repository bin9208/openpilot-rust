//! Original screenrecord feature; FFmpeg is an external runtime provider.
pub mod catalog;
mod ffmpeg;
mod http;
use crate::{config::Config, Value};
pub use http::{handle, matches};
use num_traits::ToPrimitive;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("{message}")]
    Http { status: u16, message: String },
    #[error("screenrecord operation failed")]
    Internal,
}
impl Failure {
    pub fn http(status: u16, message: &str) -> Self {
        Self::Http {
            status,
            message: message.into(),
        }
    }
}

struct Cache {
    time: f64,
    videos: Vec<Value>,
}
pub struct Screenrecord {
    pub directories: Vec<PathBuf>,
    pub cache: PathBuf,
    pub ffmpeg: PathBuf,
    wall: Option<i64>,
    monotonic: Mutex<Option<f64>>,
    videos: Mutex<Cache>,
}

pub fn find_file(directories: &[PathBuf], id: &str, wall: i64) -> Result<PathBuf, Failure> {
    let id = id.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
    if id.is_empty() || id.contains(['/', '\\']) || id.chars().count() > 64 {
        return Err(Failure::http(400, "bad file id"));
    }
    for item in catalog::build_videos(directories, wall) {
        if item.get("id").text_eq(id) {
            let folder = catalog::path_text(item.get("folder")).ok_or(Failure::Internal)?;
            let name = catalog::path_text(item.get("name")).ok_or(Failure::Internal)?;
            let path =
                catalog::absolute(&Path::new(&folder).join(name)).map_err(|_| Failure::Internal)?;
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(Failure::http(404, "screen recording not found"))
}

impl Screenrecord {
    pub fn original(config: &Config) -> Arc<Self> {
        Self::for_test(
            catalog::DIRECTORIES.iter().map(PathBuf::from).collect(),
            config
                .state
                .parent()
                .unwrap_or(&config.state)
                .join("cache/dashcam"),
            "ffmpeg".into(),
            None,
            None,
        )
    }
    pub fn for_test(
        directories: Vec<PathBuf>,
        cache: PathBuf,
        ffmpeg: PathBuf,
        wall: Option<i64>,
        monotonic: Option<f64>,
    ) -> Arc<Self> {
        Arc::new(Self {
            directories,
            cache,
            ffmpeg,
            wall,
            monotonic: Mutex::new(monotonic),
            videos: Mutex::new(Cache {
                time: 0.,
                videos: Vec::new(),
            }),
        })
    }
    pub fn wall(&self) -> i64 {
        self.wall.unwrap_or_else(|| {
            let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(time) => time.as_secs_f64(),
                Err(error) => -error.duration().as_secs_f64(),
            };
            Value::Float(seconds)
                .int()
                .ok()
                .and_then(|number| number.to_i64())
                .unwrap_or(0)
        })
    }
    pub fn set_monotonic(&self, now: f64) -> Result<(), Failure> {
        *self.monotonic.lock().map_err(|_| Failure::Internal)? = Some(now);
        Ok(())
    }
    fn now(&self) -> Result<f64, Failure> {
        if let Some(now) = *self.monotonic.lock().map_err(|_| Failure::Internal)? {
            return Ok(now);
        }
        let clock = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        Ok(clock.tv_sec.to_f64().ok_or(Failure::Internal)?
            + clock.tv_nsec.to_f64().ok_or(Failure::Internal)? / 1e9)
    }
    pub fn build_videos(&self) -> Vec<Value> {
        catalog::build_videos(&self.directories, self.wall())
    }
    pub fn cached_videos(&self) -> Result<Vec<Value>, Failure> {
        let now = self.now()?;
        {
            let cached = self.videos.lock().map_err(|_| Failure::Internal)?;
            if now - cached.time < 3. {
                return Ok(cached.videos.clone());
            }
        }
        let videos = self.build_videos();
        let mut cached = self.videos.lock().map_err(|_| Failure::Internal)?;
        cached.time = self.now()?;
        cached.videos = videos.clone();
        Ok(videos)
    }
    pub fn find_file(&self, id: &str) -> Result<PathBuf, Failure> {
        find_file(&self.directories, id, self.wall())
    }
    pub fn thumbnail(&self, id: &str) -> Result<PathBuf, Failure> {
        ffmpeg::thumbnail(
            &self.directories,
            &self.cache,
            &self.ffmpeg,
            id,
            self.wall(),
        )
    }
}
