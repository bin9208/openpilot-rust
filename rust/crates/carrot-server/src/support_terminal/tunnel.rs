use super::{lines::Lines, tunnel_binary};
use crate::Error;
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::watch, task::JoinSet};

pub(super) struct Tunnel {
    pub url: String,
    child: Option<CapturedChild>,
    readers: JoinSet<Result<(), Error>>,
}
fn find_url(line: &str) -> Option<String> {
    for (index, _) in line.match_indices("https://") {
        let tail = &line[index + 8..];
        let end = tail
            .find(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
            })
            .unwrap_or(tail.len());
        let host = &tail[..end];
        if let Some(end) = host.rfind(".trycloudflare.com").filter(|end| *end > 0) {
            return Some(format!(
                "https://{}",
                &host[..end + ".trycloudflare.com".len()]
            ));
        }
    }
    None
}
async fn drain(
    mut stream: Lines,
    label: &str,
    log: Arc<Mutex<Vec<String>>>,
    ready: watch::Sender<Option<String>>,
) -> Result<(), Error> {
    while let Some(line) = stream.next().await? {
        if !line.is_empty() {
            if let Ok(mut log) = log.lock() {
                log.push(format!("{label}: {line}"));
                let excess = log.len().saturating_sub(80);
                log.drain(..excess);
            }
            if let Some(url) = find_url(&line) {
                ready.send_if_modified(|current| {
                    if current.is_none() {
                        *current = Some(url);
                        true
                    } else {
                        false
                    }
                });
            }
        }
    }
    Ok(())
}
impl Tunnel {
    pub async fn start(
        config: &super::Config,
        binary: &tunnel_binary::Config,
        client: &reqwest::Client,
        origin: &str,
    ) -> Result<Self, Error> {
        let fake = std::env::var("CARROT_SUPPORT_FAKE_TUNNEL_URL").unwrap_or_default();
        let fake = fake.trim_matches(crate::tools::text::whitespace);
        if !fake.is_empty() {
            return Ok(Self {
                url: fake.into(),
                child: None,
                readers: JoinSet::new(),
            });
        }
        let program = binary.resolve(client).await?;
        let request = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: std::env::current_dir()?,
            argv: vec![
                program.into_os_string(),
                "tunnel".into(),
                "--url".into(),
                origin.into(),
            ],
        };
        let child = request
            .spawn()
            .map_err(|error| Error::Source(error.to_string()))?;
        let mut handle = Self {
            url: String::new(),
            child: Some(child),
            readers: JoinSet::new(),
        };
        let child = handle
            .child
            .as_mut()
            .ok_or_else(|| Error::Source("tunnel child missing".into()))?;
        let out = Lines::new(
            child
                .process
                .stdout
                .take()
                .ok_or_else(|| Error::Source("tunnel stdout missing".into()))?,
        )?;
        let err = Lines::new(
            child
                .process
                .stderr
                .take()
                .ok_or_else(|| Error::Source("tunnel stderr missing".into()))?,
        )?;
        let log = Arc::new(Mutex::new(Vec::new()));
        let (ready, mut found) = watch::channel(None);
        handle
            .readers
            .spawn_local(drain(out, "stdout", Arc::clone(&log), ready.clone()));
        handle
            .readers
            .spawn_local(drain(err, "stderr", Arc::clone(&log), ready));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            while let Some(joined) = handle.readers.try_join_next() {
                let result = joined
                    .map_err(|error| Error::Source(error.to_string()))
                    .and_then(|result| result);
                if let Err(error) = result {
                    handle.stop().await?;
                    return Err(error);
                }
            }
            if let Some(url) = found.borrow().clone() {
                handle.url = url;
                return Ok(handle);
            }
            if handle
                .child
                .as_mut()
                .is_some_and(|child| child.process.try_wait().ok().flatten().is_some())
            {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                joined = handle.readers.join_next(), if !handle.readers.is_empty() => {
                    if let Some(joined) = joined {
                        let result = joined.map_err(|error| Error::Source(error.to_string())).and_then(|result| result);
                        if let Err(error) = result {
                            handle.stop().await?;
                            return Err(error);
                        }
                    }
                }
                _ = found.changed() => {},
                _ = tokio::time::sleep(Duration::from_millis(10)) => {},
            }
        }
        let detail = log
            .lock()
            .map(|log| {
                log.iter()
                    .skip(log.len().saturating_sub(6))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        handle.stop().await?;
        Err(Error::Source(format!(
            "cloudflared did not provide a trycloudflare URL{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        )))
    }
    pub async fn stop(&mut self) -> Result<(), Error> {
        self.readers.abort_all();
        while self.readers.join_next().await.is_some() {}
        let Some(child) = &mut self.child else {
            return Ok(());
        };
        if child.process.try_wait()?.is_some() {
            self.child.take();
            return Ok(());
        }
        if let Some(pid) = rustix::process::Pid::from_raw(
            i32::try_from(child.process.id())
                .map_err(|_| Error::Source("tunnel PID range".into()))?,
        ) {
            match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
        while child.process.try_wait()?.is_none() {
            if tokio::time::Instant::now() >= deadline {
                child.process.kill()?;
                child.process.wait()?;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        self.child.take();
        Ok(())
    }
}
impl Drop for Tunnel {
    fn drop(&mut self) {
        self.readers.abort_all();
        if let Some(child) = &mut self.child {
            match child.process.kill() {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
                Err(error) => eprintln!("Support tunnel force: {error}"),
            }
            if let Err(error) = child.process.wait() {
                eprintln!("Support tunnel reap: {error}");
            }
        }
    }
}
