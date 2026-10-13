use super::{Failure, Service, State, Status, FETCH_TIMEOUT, GIT_TIMEOUT};
use std::{collections::HashSet, fs::File, sync::Arc, time::Duration};
use tokio::sync::watch;

pub(super) struct Context<'a> {
    pub service: &'a Service,
    pub lock: Arc<File>,
    pub stopped: watch::Receiver<bool>,
}

struct Tracking {
    remote: String,
    branch: String,
    upstream: String,
}

impl Context<'_> {
    async fn git(&self, args: &[&str], timeout: Duration) -> Result<(i32, String), Failure> {
        match super::commands::run(self, args, timeout).await {
            Ok(output) => Ok(output),
            Err(Failure::Cancelled) => Err(Failure::Cancelled),
            Err(error) => Ok((1, error.to_string())),
        }
    }

    async fn text(&self, args: &[&str]) -> Result<String, Failure> {
        let (code, output) = self.git(args, GIT_TIMEOUT).await?;
        Ok(if code == 0 { output } else { String::new() })
    }

    async fn tracking(&self, branch: &str) -> Result<Tracking, Failure> {
        let mut result = Tracking {
            remote: String::new(),
            branch: String::new(),
            upstream: String::new(),
        };
        if !branch.is_empty() {
            result.remote = self
                .text(&["config", "--get", &format!("branch.{branch}.remote")])
                .await?;
            let output = self
                .text(&["config", "--get-all", &format!("branch.{branch}.merge")])
                .await?;
            let refs: Vec<_> = output.lines().collect();
            let merge_ref = if refs.iter().collect::<HashSet<_>>().len() > 1 {
                format!("refs/heads/{branch}")
            } else {
                refs.first().copied().unwrap_or("").into()
            };
            if !result.remote.is_empty() {
                if let Some(remote_branch) = merge_ref.strip_prefix("refs/heads/") {
                    result.branch = remote_branch.into();
                    result.upstream = format!("{}/{}", result.remote, result.branch);
                }
            }
        }
        if result.upstream.is_empty() {
            result.upstream = self
                .text(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
                .await?;
            if let Some((remote, branch)) = result.upstream.split_once('/') {
                result.remote = remote.into();
                result.branch = branch.into();
            }
        }
        if result.remote.is_empty() {
            let output = self.text(&["remote"]).await?;
            let remotes: Vec<_> = output.split_whitespace().collect();
            result.remote = if remotes.contains(&"origin") {
                "origin".into()
            } else {
                remotes.first().copied().unwrap_or("").into()
            };
        }
        if !result.remote.is_empty() && result.branch.is_empty() && !branch.is_empty() {
            result.branch = branch.into();
            if result.upstream.is_empty() {
                result.upstream = format!("{}/{}", result.remote, result.branch);
            }
        }
        Ok(result)
    }

    pub async fn read(&self) -> Result<Status, Failure> {
        let (code, inside) = self
            .git(&["rev-parse", "--is-inside-work-tree"], GIT_TIMEOUT)
            .await?;
        if code != 0 || inside.to_lowercase() != "true" {
            return Status::error("not a git repository", (self.service.now)());
        }
        let head = self.text(&["rev-parse", "HEAD"]).await?;
        let mut branch = self.text(&["branch", "--show-current"]).await?;
        if branch.is_empty() {
            branch = self.text(&["rev-parse", "--short", "HEAD"]).await?;
        }
        let tracking = self.tracking(&branch).await?;
        let (fetch_code, fetch_output) =
            if !tracking.remote.is_empty() && !tracking.branch.is_empty() {
                let refspec = format!(
                    "+refs/heads/{}:refs/remotes/{}/{}",
                    tracking.branch, tracking.remote, tracking.branch
                );
                self.git(
                    &["fetch", "--quiet", &tracking.remote, &refspec],
                    FETCH_TIMEOUT,
                )
                .await?
            } else {
                (0, String::new())
            };
        if tracking.upstream.is_empty() {
            let mut status = Status::error("no upstream branch", (self.service.now)())?;
            status.state = State::NoUpstream;
            status.branch = branch;
            status.head = Some(head);
            status.target_head = Some(String::new());
            status.upstream = tracking.upstream;
            status.remote = Some(tracking.remote);
            status.remote_branch = Some(tracking.branch);
            status.fetch_error = Some(if fetch_code != 0 {
                fetch_output
            } else {
                String::new()
            });
            return Ok(status);
        }
        let range = format!("HEAD...{}", tracking.upstream);
        let (code, counts) = self
            .git(
                &["rev-list", "--left-right", "--count", &range],
                GIT_TIMEOUT,
            )
            .await?;
        if code != 0 {
            let mut status = Status::error(
                if counts.is_empty() {
                    "failed to compare git refs"
                } else {
                    &counts
                },
                (self.service.now)(),
            )?;
            status.branch = branch;
            status.upstream = tracking.upstream;
            return Ok(status);
        }
        let mut counts = counts.split_whitespace();
        let ahead = counts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let behind = counts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let target = self.text(&["rev-parse", &tracking.upstream]).await?;
        let mut status = Status::error(
            if fetch_code != 0 {
                fetch_output
            } else {
                String::new()
            },
            (self.service.now)(),
        )?;
        status.available = fetch_code == 0;
        status.state = if fetch_code == 0 {
            State::Ok
        } else {
            State::FetchError
        };
        status.ahead = ahead;
        status.behind = behind;
        status.branch = branch;
        status.head = Some(head);
        status.target_head = Some(target);
        status.upstream = tracking.upstream;
        status.remote = Some(tracking.remote);
        status.remote_branch = Some(tracking.branch);
        Ok(status)
    }
}
