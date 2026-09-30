#![forbid(unsafe_code)]

mod capture;

use openpilot_logmessaged::JsonValue;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const UPDATE_CHECK_INTERVAL: f64 = 5.0;

pub fn read_checkout_commit(repo: &Path, launcher: &Path) -> Option<String> {
    let git_exists = match repo.join(".git").try_exists() {
        Ok(exists) => exists,
        Err(error) if matches!(error.raw_os_error(), Some(9 | 40)) => false,
        Err(_) => return None,
    };
    let commit = if git_exists {
        match capture::git_commit(repo, launcher) {
            Ok(commit) => commit,
            Err(_) => return None,
        }
    } else {
        let source = match fs::read_to_string(repo.join("build.json")) {
            Ok(source) => source,
            Err(_) => return None,
        };
        let metadata = match JsonValue::parse(&source) {
            Ok(metadata) => metadata,
            Err(_) => return None,
        };
        metadata.get("openpilot")?.get("git_commit")?.to_utf8()?
    };
    if matches!(commit.len(), 40 | 64) && commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(commit.to_ascii_lowercase())
    } else {
        None
    }
}

pub struct UpdateStatus {
    repo: PathBuf,
    launcher: PathBuf,
    running_commit: Option<String>,
    reboot_required: bool,
    candidate_commit: Option<String>,
    next_check: f64,
}

impl UpdateStatus {
    pub fn new(repo: impl Into<PathBuf>, launcher: impl Into<PathBuf>) -> Self {
        let repo = repo.into();
        let launcher = launcher.into();
        let running_commit = read_checkout_commit(&repo, &launcher);
        Self {
            repo,
            launcher,
            running_commit,
            reboot_required: false,
            candidate_commit: None,
            next_check: 0.0,
        }
    }

    pub fn running_commit(&self) -> Option<&str> {
        self.running_commit.as_deref()
    }

    pub fn reboot_required(&self) -> bool {
        self.reboot_required
    }

    pub fn update(&mut self, now: f64) -> bool {
        if now < self.next_check {
            return self.reboot_required;
        }
        self.next_check = now + UPDATE_CHECK_INTERVAL;
        let installed_commit = read_checkout_commit(&self.repo, &self.launcher);
        let changed = self.running_commit.is_some()
            && installed_commit.is_some()
            && installed_commit != self.running_commit;
        self.reboot_required = changed && installed_commit == self.candidate_commit;
        self.candidate_commit = if changed { installed_commit } else { None };
        self.reboot_required
    }
}
