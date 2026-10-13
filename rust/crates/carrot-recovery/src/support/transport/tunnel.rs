use super::*;
use std::{
    io::Read,
    sync::mpsc::{self, RecvTimeoutError, SyncSender},
    thread::JoinHandle,
};

pub struct Tunnel {
    pub url: String,
    child: Option<CapturedChild>,
    drainer: Option<JoinHandle<Result<(), Error>>>,
}

fn find_url(line: &str) -> Option<String> {
    for (start, _) in line.match_indices("https://") {
        let tail = &line[start + 8..];
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

fn drain(mut pipe: std::io::PipeReader, ready: SyncSender<String>) -> Result<(), Error> {
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut found = false;
    loop {
        let count = pipe.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        if found {
            continue;
        }
        pending.extend_from_slice(&buffer[..count]);
        if let Some(url) = find_url(&String::from_utf8_lossy(&pending)) {
            match ready.send(url) {
                Ok(()) => {}
                Err(_) => return Ok(()),
            }
            found = true;
            pending.clear();
        } else if pending.len() > 64 * 1024 {
            pending.drain(..pending.len() - 64 * 1024);
        }
    }
}

fn exited(child: &CapturedChild) -> Result<bool, Error> {
    let pid = rustix::process::Pid::from_raw(
        i32::try_from(child.process.id())
            .map_err(|_| Error::Source("cloudflared PID range".into()))?,
    )
    .ok_or_else(|| Error::Source("cloudflared PID zero".into()))?;
    loop {
        match rustix::process::waitid(
            rustix::process::WaitId::Pid(pid),
            rustix::process::WaitIdOptions::EXITED
                | rustix::process::WaitIdOptions::NOHANG
                | rustix::process::WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => return Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
    }
}

impl Tunnel {
    pub fn start(config: &Config, local_url: &str, stop: Arc<AtomicBool>) -> Result<Self, Error> {
        let fake = std::env::var("CARROT_SUPPORT_FAKE_TUNNEL_URL").unwrap_or_default();
        let fake = helpers::strip(&fake);
        if !fake.is_empty() {
            return Ok(Self {
                url: fake.into(),
                child: None,
                drainer: None,
            });
        }
        if stop.load(Ordering::SeqCst) {
            return Err(Error::Source("support session stopped".into()));
        }
        let program = binary::resolve(config)?;
        let command = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: config.repository.clone(),
            argv: vec![
                program.into_os_string(),
                "tunnel".into(),
                "--url".into(),
                local_url.into(),
            ],
        };
        let (child, pipe) = command.spawn_session_merged_with_env(&[])?;
        let mut tunnel = Self {
            url: String::new(),
            child: Some(child),
            drainer: None,
        };
        let (ready, receiver) = mpsc::sync_channel(1);
        tunnel.drainer = Some(
            thread::Builder::new()
                .name("recovery-cloudflared-output".into())
                .spawn(move || drain(pipe, ready))?,
        );
        let started = Instant::now();
        loop {
            if stop.load(Ordering::SeqCst) {
                return Err(Error::Source("support session stopped".into()));
            }
            if started.elapsed() >= Duration::from_secs(25) {
                break;
            }
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(url) => {
                    tunnel.url = url;
                    return Ok(tunnel);
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if let Some(child) = &tunnel.child {
                if exited(child)? {
                    break;
                }
            }
        }
        Err(Error::Source(
            "cloudflared did not provide a trycloudflare URL".into(),
        ))
    }

    fn stop(&mut self) -> Result<(), Error> {
        if let Some(child) = &mut self.child {
            let pid = rustix::process::Pid::from_raw(
                i32::try_from(child.process.id())
                    .map_err(|_| Error::Source("cloudflared PID range".into()))?,
            )
            .ok_or_else(|| Error::Source("cloudflared PID zero".into()))?;
            match rustix::process::kill_process_group(pid, rustix::process::Signal::TERM) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
            let started = Instant::now();
            while !exited(child)? && started.elapsed() < Duration::from_secs(4) {
                thread::sleep(Duration::from_millis(5));
            }
            match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
            // Reserve the leader until same-session children in other groups are reaped.
            let descendants = crate::terminal::session::cleanup(pid);
            child.process.wait()?;
            self.child.take();
            descendants?;
        }
        if let Some(drainer) = self.drainer.take() {
            drainer
                .join()
                .map_err(|_| Error::Source("cloudflared drainer panicked".into()))??;
        }
        Ok(())
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("[recovery] tunnel cleanup: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::process::{Pid, WaitId, WaitIdOptions};
    use std::{os::unix::fs::PermissionsExt, process::Command};

    #[test]
    fn drop_reaps_same_session_descendant_outside_leader_group() {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).unwrap();
        let root = std::env::var_os("RECOVERY_SUPPORT_EVIDENCE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "tunnel-independent-group-{:016x}",
                u64::from_ne_bytes(random)
            ));
        fs::create_dir_all(&root).unwrap();
        let binary = root.join("cloudflared");
        fs::write(&binary, "#!/bin/bash\nset -m\nprintf '%s' $$ > leader.pid\n(trap '' TERM HUP; printf '%s' \"$BASHPID\" > descendant.pid; exec sleep 600) &\nprintf '%s\\n' 'https://owned-regression.trycloudflare.com'\nwait\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let stdout = fs::File::create(root.join("stdout.txt")).unwrap();
        let stderr = fs::File::create(root.join("stderr.txt")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "support::transport::tunnel::tests::child_driver",
                "--ignored",
                "--nocapture",
            ])
            .env("RECOVERY_TUNNEL_GROUP_ROOT", &root)
            .env("CARROT_CLOUDFLARED_BIN", &binary)
            .env("CARROT_SUPPORT_FAKE_TUNNEL_URL", "")
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                timed_out = true;
                let descendant: i32 = fs::read_to_string(root.join("descendant.pid"))
                    .unwrap()
                    .parse()
                    .unwrap();
                rustix::process::kill_process(
                    Pid::from_raw(descendant).unwrap(),
                    rustix::process::Signal::KILL,
                )
                .unwrap();
                break child.wait().unwrap();
            }
            thread::sleep(Duration::from_millis(10));
        };
        fs::write(root.join("invocation.json"), format!("{{\"scenario\":\"tunnel-independent-group\",\"timed_out\":{timed_out},\"exit\":{}}}", status.code().unwrap_or(-1))).unwrap();
        println!("OWNED_TUNNEL_GROUP_FIXTURE={}", root.display());
        assert!(
            !timed_out,
            "Tunnel drop blocked on an owned independent-group descendant; evidence {}",
            root.display()
        );
        assert!(
            status.success(),
            "{}",
            fs::read_to_string(root.join("stderr.txt")).unwrap()
        );
        if std::env::var_os("RECOVERY_SUPPORT_EVIDENCE_DIR").is_none() {
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    #[ignore = "isolated process for the owned tunnel descendant scenario"]
    fn child_driver() {
        let root = PathBuf::from(std::env::var_os("RECOVERY_TUNNEL_GROUP_ROOT").unwrap());
        rustix::process::set_child_subreaper(Some(Pid::INIT)).unwrap();
        let config = Config {
            repository: root.clone(),
            web: root.join("web"),
            data: root.join("data"),
            params: root.join("params"),
            tmux_log: root.join("tmux.log"),
            launcher: std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("openpilot-process-child"),
        };
        let mut unrelated = Command::new("/bin/sleep").arg("600").spawn().unwrap();
        let tunnel = Tunnel::start(
            &config,
            "http://127.0.0.1:1",
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !root.join("descendant.pid").exists() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        let leader: i32 = fs::read_to_string(root.join("leader.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let descendant: i32 = fs::read_to_string(root.join("descendant.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let pid = Pid::from_raw(leader).unwrap();
        let descendant_pid = Pid::from_raw(descendant).unwrap();
        assert_eq!(rustix::process::getsid(Some(descendant_pid)).unwrap(), pid);
        assert_ne!(rustix::process::getpgid(Some(descendant_pid)).unwrap(), pid);
        rustix::process::kill_process(pid, rustix::process::Signal::KILL).unwrap();
        rustix::process::waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .unwrap();
        let reserved = fs::read_to_string(format!("/proc/{leader}/stat")).unwrap();
        assert_eq!(
            reserved
                .rsplit_once(')')
                .unwrap()
                .1
                .split_whitespace()
                .next(),
            Some("Z")
        );
        drop(tunnel);
        let unrelated_alive = unrelated.try_wait().unwrap().is_none();
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
        let leader_reaped = !PathBuf::from(format!("/proc/{leader}")).exists();
        let descendant_reaped = !PathBuf::from(format!("/proc/{descendant}")).exists();
        fs::write(root.join("observation.json"), format!("{{\"leader\":{leader},\"descendant\":{descendant},\"leader_reserved_state\":\"Z\",\"leader_reaped\":{leader_reaped},\"descendant_reaped\":{descendant_reaped},\"unrelated_alive\":{unrelated_alive}}}")).unwrap();
        assert!(leader_reaped && descendant_reaped && unrelated_alive);
    }
}
