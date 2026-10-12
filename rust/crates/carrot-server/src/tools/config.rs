use crate::{auto_update_pull::runtime::Runtime, git_status};
use openpilot_params::Params;
use std::{path::PathBuf, sync::Arc};

pub struct Paths {
    pub repository: PathBuf,
    pub lock: PathBuf,
    pub launcher: PathBuf,
    pub videos: PathBuf,
    pub logs: PathBuf,
    pub calibration: [PathBuf; 2],
    pub tmux_log: PathBuf,
    pub backup: PathBuf,
    pub history: PathBuf,
}
pub struct Config {
    pub paths: Paths,
    pub params: Option<Params>,
    pub git_status: Option<Arc<git_status::Service>>,
    pub auto_update: Option<Arc<Runtime>>,
}
impl Config {
    pub fn original(app: &crate::http::Application) -> Self {
        let params = app
            .params
            .lock()
            .ok()
            .and_then(|params| params.native_params().cloned());
        let launcher = std::env::current_exe()
            .unwrap_or_default()
            .with_file_name("openpilot-process-child");
        Self {
            paths: Paths {
                repository: "/data/openpilot".into(),
                lock: std::env::var_os("CARROT_REPO_LOCK_PATH")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "/tmp/carrot_repo_update.lock".into()),
                launcher,
                videos: "/data/media/0/videos".into(),
                logs: "/data/media/0/realdata".into(),
                calibration: [
                    "/data/params/d_tmp/CalibrationParams".into(),
                    "/data/params/d/CalibrationParams".into(),
                ],
                tmux_log: "/data/media/tmux.log".into(),
                backup: app.config.params_backup.clone(),
                history: app.config.state.join("tool_jobs.json"),
            },
            params,
            git_status: app.git_status.clone(),
            auto_update: app.auto_update.clone(),
        }
    }
}
