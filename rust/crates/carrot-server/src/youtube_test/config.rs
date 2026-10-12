use crate::{youtube_live::profiles, Error};
use openpilot_params::Params;
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone)]
pub struct Paths {
    pub state: PathBuf,
    pub log: PathBuf,
    pub report: PathBuf,
    pub live_state: PathBuf,
    pub secret: PathBuf,
}
#[derive(Clone)]
pub struct CommandSpec {
    pub path: PathBuf,
    pub args: Vec<OsString>,
    pub pattern: String,
}
#[derive(Clone)]
pub struct Config {
    pub repository: PathBuf,
    pub params: Params,
    pub paths: Paths,
    pub camera: CommandSpec,
    pub encoder: PathBuf,
    pub runner: CommandSpec,
    pub launcher: PathBuf,
    pub status_url: String,
}
impl Config {
    pub fn original() -> Result<Self, Error> {
        let repository = crate::config::runtime_repository()?;
        let binary = std::env::current_exe()?;
        let siblings = binary
            .parent()
            .ok_or_else(|| Error::Source("native executable directory missing".into()))?;
        let data = crate::config::Config::from_environment(&repository);
        let runner = siblings.join("openpilot-youtube-test");
        let camera = siblings.join("openpilot-camerad");
        Ok(Self {
            repository,
            params: Params::for_runtime()?,
            paths: Paths {
                state: "/tmp/carrot-youtube-test-state.json".into(),
                log: "/tmp/carrot-youtube-test.log".into(),
                report: "/tmp/carrot-youtube-test-report.json".into(),
                live_state: data.state.join("youtube_live.json"),
                secret: data.state.join("youtube_live_secret.json"),
            },
            camera: CommandSpec {
                pattern: camera.to_string_lossy().into_owned(),
                path: camera,
                args: Vec::new(),
            },
            encoder: siblings.join("openpilot-encoderd"),
            runner: CommandSpec {
                pattern: runner.to_string_lossy().into_owned(),
                path: runner,
                args: Vec::new(),
            },
            launcher: siblings.join("openpilot-process-child"),
            status_url: "http://127.0.0.1:7000/api/youtube_live/status".into(),
        })
    }
    pub(super) fn encoder(&self, quality: i32) -> CommandSpec {
        let profile = profiles::selected(quality);
        CommandSpec {
            path: self.encoder.clone(),
            args: vec![profile.encoder_flag.into()],
            pattern: format!("{}\0{}\0", self.encoder.display(), profile.encoder_flag),
        }
    }
    pub(super) fn child_specs(&self, quality: i32) -> [(String, CommandSpec); 2] {
        [
            ("camerad".into(), self.camera.clone()),
            (
                profiles::selected(quality).process.into(),
                self.encoder(quality),
            ),
        ]
    }
}
