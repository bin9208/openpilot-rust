use crate::{param_changes::text, Error, Value};
use chrono::{DateTime, SecondsFormat, Utc};
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime},
};

pub struct Git {
    pub repository: PathBuf,
    pub program: PathBuf,
}
#[derive(Clone)]
pub struct Creation {
    pub id: String,
    pub now: String,
    pub meta: Value,
}
pub fn now_iso() -> String {
    DateTime::<Utc>::from(SystemTime::now()).to_rfc3339_opts(SecondsFormat::Secs, false)
}
impl Creation {
    pub fn capture(git: &Git) -> Self {
        let now = now_iso();
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            now,
            meta: git.metadata(),
        }
    }
}

pub fn commit_url(remote: &Value, commit: &Value) -> Result<Value, Error> {
    let remote = text::stripped(remote, true)?.string()?;
    let commit = text::stripped(commit, true)?.string()?;
    if remote.is_empty() || commit.is_empty() {
        return Ok(Value::text(""));
    }
    let (tail, http) = match remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("http://github.com/"))
    {
        Some(tail) => (tail, true),
        None => match remote.strip_prefix("git@github.com:") {
            Some(tail) => (tail, false),
            None => return Ok(Value::text("")),
        },
    };
    let Some((owner, raw_repository)) = tail.split_once('/') else {
        return Ok(Value::text(""));
    };
    let repository = if http {
        raw_repository.strip_suffix('/').unwrap_or(raw_repository)
    } else {
        raw_repository
    };
    if owner.is_empty() || repository.is_empty() || (http && repository.contains(['/', '#', '?'])) {
        return Ok(Value::text(""));
    }
    let repository = repository
        .strip_suffix(".git")
        .filter(|repository| !repository.is_empty())
        .unwrap_or(repository);
    Ok(Value::text(&format!(
        "https://github.com/{owner}/{repository}/commit/{commit}"
    )))
}
impl Git {
    fn command(&self, args: &[&str]) -> String {
        let result = (|| -> std::io::Result<String> {
            let mut child = Command::new(&self.program)
                .args(args)
                .current_dir(&self.repository)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let mut stdout = child
                .stdout
                .take()
                .ok_or_else(|| std::io::Error::other("missing Git stdout pipe"))?;
            std::thread::scope(|scope| {
                let reader = scope.spawn(move || {
                    let mut output = Vec::new();
                    stdout.read_to_end(&mut output)?;
                    Ok::<_, std::io::Error>(output)
                });
                let started = Instant::now();
                let status = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break status.success(),
                        Ok(None) => {}
                        Err(error) => {
                            child.kill()?;
                            child.wait()?;
                            return Err(error);
                        }
                    }
                    if started.elapsed() >= Duration::from_secs(3) {
                        child.kill()?;
                        child.wait()?;
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                let output = reader
                    .join()
                    .map_err(|_| std::io::Error::other("Git stdout reader failed"))??;
                if !status {
                    return Ok(String::new());
                }
                let output = String::from_utf8_lossy(&output);
                let points = output.chars().map(u32::from).collect::<Vec<_>>();
                Ok(crate::state::trim(&points)
                    .iter()
                    .copied()
                    .filter_map(char::from_u32)
                    .collect())
            })
        })();
        result.unwrap_or_default()
    }
    pub fn metadata(&self) -> Value {
        let branch = self.command(&["branch", "--show-current"]);
        let commit = self.command(&["rev-parse", "HEAD"]);
        let date = self.command(&["show", "-s", "--format=%cI", "HEAD"]);
        let remote = self.command(&["config", "--get", "remote.origin.url"]);
        let short = commit.chars().take(7).collect::<String>();
        let url = commit_url(&Value::text(&remote), &Value::text(&commit))
            .unwrap_or_else(|_| Value::text(""));
        Value::object([
            ("branch", Value::text(&branch)),
            ("commit", Value::text(&commit)),
            ("commit_short", Value::text(&short)),
            ("commit_date", Value::text(&date)),
            ("remote", Value::text(&remote)),
            ("commit_url", url),
        ])
    }
}
