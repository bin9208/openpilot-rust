use crate::models::{GitBranchStatus, GitBranchStatusState as State};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub trait GitCommands {
    fn run(&mut self, repo: &Path, args: &[&str], timeout: Duration) -> GitOutput;
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct GitOutput {
    pub code: i32,
    pub stdout: String,
}

fn trim(value: &str) -> &str {
    value.trim_matches(openpilot_startup_ui::text::whitespace)
}

#[must_use]
pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    let start = std::path::absolute(start).ok()?;
    start
        .ancestors()
        .filter_map(|path| fs::canonicalize(path).ok())
        .find(|path| path.join(".git").exists())
}

#[must_use]
pub fn resolve_git_dir(repo: &Path) -> Option<PathBuf> {
    let path = repo.join(".git");
    if path.is_dir() {
        return Some(path);
    }
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let text = trim(&text);
    if !text.get(..7)?.eq_ignore_ascii_case("gitdir:") {
        return None;
    }
    let target = Path::new(trim(&text[7..]));
    Some(if target.is_absolute() {
        target.to_owned()
    } else {
        repo.join(target)
    })
}

#[must_use]
pub fn read_head_branch(repo: &Path) -> Option<String> {
    let bytes = fs::read(resolve_git_dir(repo)?.join("HEAD")).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let text = trim(&text);
    if let Some(branch) = text.strip_prefix("ref: refs/heads/") {
        return Some(branch.into());
    }
    (!text.is_empty()).then(|| text.chars().take(12).collect())
}

fn status(branch: impl Into<String>, state: State, detail: impl Into<String>) -> GitBranchStatus {
    GitBranchStatus {
        branch: branch.into(),
        state,
        detail: detail.into(),
    }
}

pub fn read_git_status(
    repo: Option<&Path>,
    commands: &mut impl GitCommands,
    timeout: Duration,
) -> GitBranchStatus {
    let Some(repo) = repo else {
        return status("git", State::Unknown, "저장소 없음");
    };
    let output = commands.run(repo, &["branch", "--show-current"], Duration::from_secs(1));
    let branch = trim(&output.stdout);
    if output.code != 0 || branch.is_empty() {
        return status(
            read_head_branch(repo).unwrap_or_else(|| "HEAD".into()),
            State::Unknown,
            "브렌치 아님",
        );
    }
    let Some((remote, remote_branch)) = tracking_branch(repo, branch, commands) else {
        return status(branch, State::Missing, "원격 설정 없음");
    };
    match commands
        .run(
            repo,
            &[
                "ls-remote",
                "--exit-code",
                "--heads",
                &remote,
                &remote_branch,
            ],
            timeout,
        )
        .code
    {
        0 => (),
        2 => return status(branch, State::Missing, "원격에서 삭제됨"),
        _ => return status(branch, State::Unknown, "원격 확인 실패"),
    }
    match behind_count(repo, &remote, &remote_branch, commands, timeout) {
        Some(count) if count > 0 => status(branch, State::Pull, format!("git pull 가능 +{count}")),
        Some(_) => status(branch, State::Ok, ""),
        None => status(branch, State::Unknown, "pull 확인 실패"),
    }
}

fn tracking_branch(
    repo: &Path,
    branch: &str,
    commands: &mut impl GitCommands,
) -> Option<(String, String)> {
    let output = commands.run(repo, &["remote"], Duration::from_secs(1));
    if output.code != 0 {
        return None;
    }
    let remotes: Vec<_> = output
        .stdout
        .lines()
        .map(trim)
        .filter(|value| !value.is_empty())
        .collect();
    if remotes.is_empty() {
        return None;
    }
    let upstream = commands.run(
        repo,
        &[
            "for-each-ref",
            "--format=%(upstream:short)",
            &format!("refs/heads/{branch}"),
        ],
        Duration::from_secs(1),
    );
    if upstream.code == 0 {
        let upstream = trim(&upstream.stdout);
        let mut ordered = remotes.clone();
        ordered.sort_by_key(|value| std::cmp::Reverse(value.chars().count()));
        for remote in ordered {
            if let Some(remote_branch) = upstream
                .strip_prefix(&format!("{remote}/"))
                .filter(|value| !value.is_empty())
            {
                return Some((remote.into(), remote_branch.into()));
            }
        }
    }
    Some((
        remotes
            .iter()
            .find(|value| **value == "origin")
            .unwrap_or(&remotes[0])
            .to_string(),
        branch.into(),
    ))
}

fn behind_count(
    repo: &Path,
    remote: &str,
    branch: &str,
    commands: &mut impl GitCommands,
    timeout: Duration,
) -> Option<i64> {
    let tracking = format!("refs/remotes/{remote}/{branch}");
    if commands
        .run(
            repo,
            &[
                "fetch",
                "--quiet",
                "--prune",
                remote,
                &format!("+refs/heads/{branch}:{tracking}"),
            ],
            timeout,
        )
        .code
        != 0
    {
        return None;
    }
    let result = commands.run(
        repo,
        &["rev-list", "--count", &format!("HEAD..{tracking}")],
        Duration::from_secs(1),
    );
    if result.code != 0 {
        return None;
    }
    trim(&result.stdout)
        .parse::<i64>()
        .ok()
        .map(|count| count.max(0))
}

pub struct GitBranchStatusProvider {
    repo: Option<PathBuf>,
    interval: f64,
    timeout: Duration,
    remote_enabled: bool,
    next: f64,
    status: Arc<Mutex<GitBranchStatus>>,
    closed: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl GitBranchStatusProvider {
    #[must_use]
    pub fn new(start: &Path, interval: f64, timeout: f64, remote_enabled: bool) -> Self {
        let repo = find_git_root(start);
        let branch = repo
            .as_deref()
            .and_then(read_head_branch)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "git".into());
        let detail = if repo.is_none() {
            "저장소 없음"
        } else if remote_enabled {
            "확인 중"
        } else {
            ""
        };
        Self {
            repo,
            interval: interval.max(5.0),
            timeout: Duration::from_secs_f64(timeout.max(0.5)),
            remote_enabled,
            next: 0.0,
            status: Arc::new(Mutex::new(status(branch, State::Unknown, detail))),
            closed: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    /// # Errors
    /// Returns OS worker creation errors or a completed worker panic.
    pub fn status(&mut self, now: f64) -> io::Result<GitBranchStatus> {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            self.join()?;
        }
        if self.remote_enabled
            && !self.closed.load(Ordering::Acquire)
            && self.repo.is_some()
            && now >= self.next
            && self.worker.is_none()
        {
            self.next = now + self.interval;
            let repo = self.repo.clone();
            let timeout = self.timeout;
            let shared = Arc::clone(&self.status);
            let closed = Arc::clone(&self.closed);
            self.worker = Some(
                thread::Builder::new()
                    .name("cluster-git-status".into())
                    .spawn(move || {
                        use openpilot_ui_application::scheduling::{Native, Platform};
                        let _ = Native { root: "/".into() }.set_other(0);
                        let mut commands = super::git_command::NativeGitCommands::new(closed);
                        let next = read_git_status(repo.as_deref(), &mut commands, timeout);
                        *shared
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
                    })?,
            );
        }
        Ok(self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
    }

    fn join(&mut self) -> io::Result<()> {
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("git status worker panicked"))?;
        }
        Ok(())
    }

    /// # Errors
    /// Returns a worker panic after cancelling any owned Git command and joining.
    pub fn close(&mut self) -> io::Result<()> {
        self.closed.store(true, Ordering::Release);
        self.join()
    }
}

impl Drop for GitBranchStatusProvider {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
