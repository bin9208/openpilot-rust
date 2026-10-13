use super::{Error, Result};
use std::collections::BTreeMap;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ToolsState {
    pub playback_s: f64,
    pub duration_s: f64,
    pub paused: bool,
    pub status_text: String,
    pub current_log_folder: String,
    pub options: BTreeMap<String, bool>,
    pub video_width: i64,
    pub video_height: i64,
    pub video_status: Option<String>,
    pub cutin_status: Option<String>,
    pub data_lines: Vec<String>,
    #[serde(skip)]
    pub video_rgba: Option<Vec<u8>>,
}
impl Default for ToolsState {
    fn default() -> Self {
        Self {
            playback_s: 0.,
            duration_s: 0.,
            paused: false,
            status_text: "Waiting for replay".into(),
            current_log_folder: "--".into(),
            options: BTreeMap::new(),
            video_width: 0,
            video_height: 0,
            video_status: None,
            cutin_status: None,
            data_lines: Vec::new(),
            video_rgba: None,
        }
    }
}
impl ToolsState {
    /// # Errors
    /// Returns an error for nonfinite replay times, which the native wire cannot encode.
    pub fn check_times(&self) -> Result<()> {
        if !self.playback_s.is_finite() || !self.duration_s.is_finite() {
            return Err(Error::Contract("nonfinite replay time"));
        }
        Ok(())
    }
    #[must_use]
    pub fn video(&self) -> Option<(&[u8], usize, usize)> {
        let width = usize::try_from(self.video_width).ok().filter(|v| *v > 0)?;
        let height = usize::try_from(self.video_height).ok().filter(|v| *v > 0)?;
        let bytes = self.video_rgba.as_deref()?;
        (width.checked_mul(height)?.checked_mul(4)? == bytes.len())
            .then_some((bytes, width, height))
    }
}
