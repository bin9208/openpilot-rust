use crate::{settings, wave, Error, Sound};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use std::path::{Path, PathBuf};
// Ordinals and filenames are retained from sound_list, including unsupported 34/36.
const SOUNDS: &[(u16, &str, Option<usize>)] = &[
    (1, "engage.wav", Some(1)),
    (2, "disengage.wav", Some(1)),
    (3, "refuse.wav", Some(1)),
    (6, "prompt.wav", Some(1)),
    (7, "prompt.wav", None),
    (8, "prompt_distracted.wav", None),
    (4, "warning_soft.wav", None),
    (5, "warning_immediate.wav", None),
    (10, "tici_engaged.wav", None),
    (11, "tici_disengaged.wav", None),
    (12, "traffic_sign_green.wav", None),
    (13, "traffic_sign_changed.wav", None),
    (19, "audio_traffic_error.wav", None),
    (20, "audio_car_watchout.wav", None),
    (14, "audio_lane_change.wav", None),
    (22, "audio_stopstop.wav", None),
    (15, "audio_stopping.wav", None),
    (16, "audio_auto_hold.wav", None),
    (17, "audio_engage.wav", None),
    (18, "audio_disengage.wav", None),
    (21, "audio_speed_down.wav", None),
    (9, "audio_turn.wav", None),
    (23, "reverse_gear.wav", Some(1)),
    (24, "audio_1.wav", None),
    (25, "audio_2.wav", None),
    (26, "audio_3.wav", None),
    (27, "audio_4.wav", None),
    (28, "audio_5.wav", None),
    (29, "audio_6.wav", None),
    (30, "audio_7.wav", None),
    (31, "audio_8.wav", None),
    (32, "audio_9.wav", None),
    (33, "audio_10.wav", None),
    (35, "prompt.wav", Some(1)),
];
pub struct Assets {
    pub root: PathBuf,
    pub engage_volume: f64,
    pub tizi: bool,
}
impl Assets {
    pub fn load(&self, language: &str, logger: &mut Logger) -> Result<Vec<Sound>, Error> {
        let folder = self.root.join(settings::directory(language));
        let fallback = self.root.join("sounds_eng");
        SOUNDS
            .iter()
            .map(|&(alert, original, loops)| {
                let filename = match (self.tizi, alert) {
                    (true, 1) => "engage_tizi.wav",
                    (true, 2) => "disengage_tizi.wav",
                    _ => original,
                };
                let path = resolve(&folder, &fallback, filename)?;
                if path.file_name().is_some_and(|name| name == "prompt.wav")
                    && filename != "prompt.wav"
                {
                    logger.emit(
                        log_site!(),
                        Record::text(
                            Level::Error,
                            format!("soundd missing asset {filename}, using prompt.wav"),
                        ),
                    )?;
                }
                let volume = if matches!(alert, 1 | 2 | 23) {
                    self.engage_volume
                } else {
                    1.
                };
                Ok(Sound {
                    alert,
                    samples: wave::load(&path, volume)?,
                    loops,
                })
            })
            .collect()
    }
}
pub fn resolve(folder: &Path, fallback: &Path, filename: &str) -> Result<PathBuf, Error> {
    for path in [
        folder.join(filename),
        fallback.join(filename),
        fallback.join("prompt.wav"),
    ] {
        if path.exists() {
            return Ok(path);
        }
    }
    Err(Error::MissingAsset(filename.into()))
}
