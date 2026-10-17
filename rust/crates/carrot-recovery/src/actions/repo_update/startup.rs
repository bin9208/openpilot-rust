//! Native failed-startup actions keep the active checkout intact until a
//! complete source-matched package is staged by the existing installer.
use super::*;
use openpilot_hardware_control::reboot;
use openpilot_startup_recovery::state::Backend;

pub struct Native {
    pub config: Config,
    pub reboot: reboot::Config,
    pub automatic: bool,
    pub pending_reboot: Option<String>,
    lock: Option<File>,
    before_reboot: Option<Box<dyn FnMut() -> Result<(), String>>>,
}
impl Native {
    pub fn new(
        config: Config,
        reboot: reboot::Config,
        automatic: bool,
        pending_reboot: Option<String>,
    ) -> Self {
        Self {
            config,
            reboot,
            automatic,
            pending_reboot,
            lock: None,
            before_reboot: None,
        }
    }
    pub fn before_reboot(mut self, callback: impl FnMut() -> Result<(), String> + 'static) -> Self {
        self.before_reboot = Some(Box::new(callback));
        self
    }
    fn lock(&self) -> Result<&File, String> {
        self.lock
            .as_ref()
            .ok_or_else(|| "Startup recovery does not own the repository lock".into())
    }
    fn prepared(code: i32) -> Result<(), String> {
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "Native source/package staging failed (exit {code}); active checkout preserved."
            ))
        }
    }
}
impl Backend for Native {
    // Set GIT_TERMINAL_PROMPT=0 in the isolated action process before it starts
    // its progress publisher; never mutate the UI process environment here.
    fn disable_prompt(&mut self) {}
    fn with_lock<T>(
        &mut self,
        run: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        if std::env::var("GIT_TERMINAL_PROMPT").as_deref() != Ok("0") {
            return Err(
                "Startup recovery requires noninteractive Git in its isolated action process."
                    .into(),
            );
        }
        self.lock = Some(acquire(&self.config).map_err(|error| match error {
            Failure::Busy(message) => message,
            Failure::Runtime(error) => error.to_string(),
        })?);
        let result = run(self);
        self.lock.take();
        result
    }
    fn pull(&mut self) -> Result<(String, bool), String> {
        let lock = self.lock()?;
        let git = |args: &[&str]| checked_git(&self.config, args, lock).map_err(|e| e.to_string());
        if git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?.is_empty() {
            return Err("Select a branch before updating.".into());
        }
        let before = git(&["rev-parse", "HEAD"])?;
        let repository = git_config::Repository {
            directory: &self.config.repository,
            launcher: &self.config.launcher,
            lock: Some(lock.as_fd()),
        };
        let (code, detail, target) =
            git_config::prepare_git_pull(&repository).map_err(|e| e.to_string())?;
        if code != 0 || target.is_empty() {
            return Err(if detail.is_empty() {
                "Unable to fetch the current branch.".into()
            } else {
                detail
            });
        }
        let changed = before != target;
        if !self.automatic || changed || self.pending_reboot.as_ref() == Some(&target) {
            Self::prepared(
                stage(&self.config, &target, None, true, lock).map_err(|e| e.to_string())?,
            )?;
        }
        Ok((target, changed))
    }
    fn request_rebuild(&mut self) -> Result<(), String> {
        // This is a verified package repair. SCons caches cannot repair native
        // artifacts and are never removed by this Rust runtime.
        Self::prepared(
            native_action(&self.config, "__stage-repair", None, self.lock()?)
                .map_err(|e| e.to_string())?,
        )
    }
    fn reboot(&mut self) -> Result<(), String> {
        self.lock()?;
        if let Some(callback) = &mut self.before_reboot {
            callback()?;
        }
        reboot::reboot(&self.reboot).map_err(|e| e.to_string())
    }
}
