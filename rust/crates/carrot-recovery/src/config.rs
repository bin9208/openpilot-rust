use crate::Error;
use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub repository: PathBuf,
    pub web: PathBuf,
    pub launcher: PathBuf,
    pub data: PathBuf,
    pub params: PathBuf,
    pub tmux_log: PathBuf,
}

impl Config {
    pub fn original() -> Result<Self, Error> {
        let executable = env::current_exe()?;
        let repository = ["OPENPILOT_ROOT", "BASEDIR"]
            .into_iter()
            .find_map(env::var_os)
            .map(PathBuf::from)
            .unwrap_or_else(|| "/data/openpilot".into());
        Ok(Self {
            web: repository.join("openpilot/selfdrive/carrot/web"),
            repository,
            launcher: executable.with_file_name("openpilot-process-child"),
            data: env::var_os("CARROT_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/data/carrot".into()),
            params: "/data/params".into(),
            tmux_log: "/data/media/tmux.log".into(),
        })
    }
}
