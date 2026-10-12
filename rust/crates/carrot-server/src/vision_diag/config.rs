use crate::{http::Application, tools::runner::Runner, vision_test, Error};
use openpilot_params::Params;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Config {
    pub repository: PathBuf,
    pub proc_root: PathBuf,
    pub vision: vision_test::Config,
    pub params_available: bool,
    pub params_root: Option<PathBuf>,
    pub launcher: PathBuf,
    pub process_matches: [(&'static str, String); 5],
}

impl Config {
    pub fn original(app: &Application) -> Result<Self, Error> {
        let vision = match &app.vision_test {
            Some(config) => config.clone(),
            None => vision_test::Config::original()?,
        };
        let current = std::env::current_exe()?;
        let params_available = app
            .params
            .lock()
            .map_err(|_| Error::Source("Params lock poisoned".into()))?
            .has_params();
        Ok(Self {
            repository: std::env::var_os("CARROT_REPO_DIR")
                .map_or_else(|| "/data/openpilot".into(), PathBuf::from),
            proc_root: "/proc".into(),
            params_available,
            params_root: vision.params_root.clone(),
            launcher: vision.launcher.clone(),
            process_matches: [
                ("camerad", vision.children[0].1.pattern.clone()),
                ("stream_encoderd", vision.children[1].1.pattern.clone()),
                (
                    "encoderd",
                    vision.children[1].1.path.to_string_lossy().into_owned(),
                ),
                ("webrtcd", vision.children[2].1.pattern.clone()),
                ("carrot_web", current.to_string_lossy().into_owned()),
            ],
            vision,
        })
    }
    pub(crate) fn params(&self) -> Result<Option<Params>, Error> {
        if !self.params_available {
            return Ok(None);
        }
        Ok(Some(match &self.params_root {
            Some(root) => Params::for_runtime_at(root)?,
            None => Params::for_runtime()?,
        }))
    }
    pub(crate) fn runner(&self) -> (Runner, tokio::sync::watch::Sender<bool>) {
        let (owner, stopped) = tokio::sync::watch::channel(false);
        (
            Runner {
                repository: self.repository.clone(),
                launcher: self.launcher.clone(),
                lock: None,
                stopped,
            },
            owner,
        )
    }
}
