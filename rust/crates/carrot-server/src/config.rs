use std::{
    env,
    fs::{self, File, FileTimes},
    path::{Path, PathBuf},
};

pub const BODY_LIMIT: usize = 16 * 1024 * 1024;
pub const UNIT_CYCLE: [u32; 6] = [1, 2, 5, 10, 50, 100];
pub const LEGACY_STATE_FILES: [&str; 7] = [
    "web_settings.json",
    "youtube_live.json",
    "youtube_live_secret.json",
    "setting_favorites.json",
    "setting_profiles.json",
    "git.json",
    "tool_jobs.json",
];

pub fn runtime_repository() -> Result<PathBuf, crate::Error> {
    for name in ["OPENPILOT_ROOT", "BASEDIR"] {
        if let Some(path) = env::var_os(name) {
            return Ok(fs::canonicalize(path)?);
        }
    }
    let executable = env::current_exe()?;
    let current = env::current_dir()?;
    for base in [executable.parent(), Some(current.as_path())]
        .into_iter()
        .flatten()
    {
        for root in base.ancestors() {
            if root.join("openpilot/selfdrive/carrot").is_dir() {
                return Ok(fs::canonicalize(root)?);
            }
        }
    }
    Err(crate::Error::Source(
        "cannot locate runtime repository from OPENPILOT_ROOT, BASEDIR, executable or working directory".into(),
    ))
}

#[derive(Clone, Debug)]
pub struct Config {
    pub repository: PathBuf,
    pub web: PathBuf,
    pub shared_assets: PathBuf,
    pub training_assets: PathBuf,
    pub settings: PathBuf,
    pub state: PathBuf,
    pub legacy_state: PathBuf,
    pub params_backup: PathBuf,
}

impl Config {
    pub fn at(repository: &Path, data: &Path, settings: &Path) -> Self {
        let selfdrive = repository.join("openpilot/selfdrive");
        Self {
            repository: repository.into(),
            web: selfdrive.join("carrot/web"),
            shared_assets: selfdrive.join("assets"),
            training_assets: selfdrive.join("assets/training"),
            settings: settings.into(),
            state: data.join("state"),
            legacy_state: PathBuf::from("/data/openpilot/openpilot/selfdrive/carrot/data/state"),
            params_backup: PathBuf::from("/data/media/params_backup.json"),
        }
    }

    pub fn from_environment(repository: &Path) -> Self {
        let settings = env::var_os("CARROT_SETTINGS_PATH").map_or_else(
            || repository.join("openpilot/selfdrive/carrot_settings.json"),
            PathBuf::from,
        );
        let data = env::var_os("CARROT_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/data/carrot".into());
        Self::at(repository, &data, &settings)
    }

    pub fn validate(&self) -> Result<(), crate::Error> {
        if !self.web.is_dir() {
            return Err(crate::Error::Source(format!(
                "web dir not found: {}",
                self.web.display()
            )));
        }
        Ok(())
    }

    pub fn migrate_legacy_state(&self) {
        let _ = self.try_migrate_legacy_state();
    }

    fn try_migrate_legacy_state(&self) -> std::io::Result<()> {
        if !self.legacy_state.is_dir() {
            return Ok(());
        }
        if fs::canonicalize(&self.legacy_state).ok() == fs::canonicalize(&self.state).ok() {
            return Ok(());
        }
        fs::create_dir_all(&self.state)?;
        for name in LEGACY_STATE_FILES {
            let source = self.legacy_state.join(name);
            let destination = self.state.join(name);
            if source.is_file() && !destination.exists() {
                fs::copy(&source, &destination)?;
                let metadata = fs::metadata(&source)?;
                let times = FileTimes::new()
                    .set_accessed(metadata.accessed()?)
                    .set_modified(metadata.modified()?);
                File::open(destination)?.set_times(times)?;
            }
        }
        Ok(())
    }
}
