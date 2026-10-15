//! Native providers used only by the device collector and its owned upload child.
use super::{
    collector::{Collector, Provider},
    store,
};
use openpilot_logging::{Fields, Value};
use std::{
    env, fs, io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub struct Native {
    pub executable: PathBuf,
    pub launcher: PathBuf,
    pub tmux: PathBuf,
}
struct UploadChild(std::process::Child);
impl Drop for UploadChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Native {
    pub fn start() -> io::Result<Collector> {
        let data = env::var_os("CARROT_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/data/carrot".into());
        Collector::start(
            data.join("process_crashes"),
            openpilot_process_supervision::crash::SOCKET_PATH.into(),
            Arc::new(Self {
                executable: env::current_exe()?,
                launcher: env::current_exe()?.with_file_name("openpilot-process-child"),
                tmux: "tmux".into(),
            }),
        )
    }
}
impl Provider for Native {
    fn capture(&self, event: &Fields) -> io::Result<String> {
        let mut argv = vec![
            self.tmux.clone().into_os_string(),
            "capture-pane".into(),
            "-p".into(),
            "-J".into(),
            "-S".into(),
            "-1000".into(),
        ];
        if let Some(pane) = store::text(event, "pane").filter(|pane| !pane.is_empty()) {
            argv.extend(["-t".into(), pane.into()]);
        }
        let result = env::current_dir()
            .map_err(openpilot_process_supervision::CaptureError::Io)
            .and_then(|cwd| {
                openpilot_process_supervision::CapturedCommand {
                    launcher: self.launcher.clone(),
                    cwd,
                    argv,
                }
                .capture(Duration::from_secs(3), None)
            });
        let failure = match result {
            Ok(output) if output.status.success() => return Ok(store::tail(&output.stdout)),
            Ok(_) => "CalledProcessError",
            Err(openpilot_process_supervision::CaptureError::Timeout { .. }) => "TimeoutExpired",
            Err(openpilot_process_supervision::CaptureError::Io(error))
            | Err(openpilot_process_supervision::CaptureError::Launch(
                openpilot_process_supervision::Error::Io(error),
            )) => match error.kind() {
                io::ErrorKind::NotFound => "FileNotFoundError",
                io::ErrorKind::PermissionDenied => "PermissionError",
                _ => "OSError",
            },
            Err(_) => "OSError",
        };
        Ok(format!(
            "Tmux capture unavailable: {failure}. Exit metadata retained.\n"
        ))
    }
    fn payload(&self, event: &Fields) -> io::Result<Fields> {
        let params = openpilot_params::Params::for_runtime().map_err(io::Error::other)?;
        let mut payload: Fields = [("tmux_why".into(), Value::Text("process_crash".into()))]
            .into_iter()
            .collect();
        for (field, key) in [
            ("car_name", "CarName"),
            ("git_branch", "GitBranch"),
            ("git_commit", "GitCommit"),
            ("git_commit_date", "GitCommitDate"),
            ("github_id", "GithubUsername"),
            ("git_remote", "GitRemote"),
            ("dongle_id", "DongleId"),
            ("device_serial", "HardwareSerial"),
        ] {
            let value = store::text(event, field)
                .filter(|v| !v.is_empty())
                .map(str::to_owned);
            let value = match value {
                Some(v) => v,
                None => params
                    .get(key)
                    .map_err(io::Error::other)?
                    .map(|v| String::from_utf8_lossy(&v).into_owned())
                    .unwrap_or_default(),
            };
            payload.insert(field.into(), Value::Text(value));
        }
        Ok(payload)
    }
    fn upload(
        &self,
        target: &str,
        _payload: &Fields,
        path: &Path,
        stop: &AtomicBool,
    ) -> io::Result<bool> {
        if stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let child = Command::new(&self.executable)
            .arg("--process-crash-upload")
            .arg(target)
            .arg(path.with_extension("json"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?;
        let mut child = UploadChild(child);
        // Keep HTTP entirely outside the collector. Cancellation kills/reaps this
        // owner even during DNS/TLS/slow response, preserving manager shutdown.
        loop {
            match child.0.try_wait() {
                Ok(Some(status)) => break Ok(status.success()),
                Err(error) => break Err(error),
                Ok(None) => {}
            }
            if stop.load(Ordering::Relaxed) {
                break Ok(false);
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

pub fn upload_child(target: &str, path: &Path) -> io::Result<bool> {
    if !matches!(target, "dsm" | "carrot_logs") {
        return Err(io::Error::other("invalid crash upload destination"));
    }
    if env::var("FAKEUPLOAD").as_deref() == Ok("1") {
        return Ok(false);
    }
    let report = store::fields(&fs::read(path)?)?;
    let Some(Value::Object(payload)) = report.get("payload") else {
        return Err(io::Error::other("invalid crash payload"));
    };
    let environment = openpilot_web_upload::Environment::for_runtime().map_err(io::Error::other)?;
    let endpoint = if target == "dsm" {
        let data = env::var_os("CARROT_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/data/carrot".into());
        let settings = fs::read(data.join("state/web_settings.json"))
            .ok()
            .and_then(|v| store::fields(&v).ok())
            .unwrap_or_default();
        let settings = upload_settings(&settings);
        let (base, mut token) = openpilot_web_upload::web_settings(&settings, &environment)
            .map_err(io::Error::other)?;
        if token.is_empty() {
            token = openpilot_web_upload::create_session_with_purpose(
                &base,
                payload,
                "tmux",
                openpilot_web_upload::SessionMode::Sync,
            )
            .map_err(io::Error::other)?;
        }
        openpilot_web_upload::tmux_target(&settings, &environment, &token)
            .map_err(io::Error::other)?
    } else {
        openpilot_web_upload::carrot_logs_target(&environment).map_err(io::Error::other)?
    };
    let response = openpilot_web_upload::TmuxUpload {
        target: &endpoint,
        payload,
        tmux_path: &path.with_extension("txt"),
        settings_path: None,
    }
    .post()
    .map_err(io::Error::other)?;
    Ok((200..300).contains(&response.status))
}

fn upload_settings(raw: &Fields) -> Fields {
    // Only this normalized setting is consumed by the existing tmux endpoints.
    // Match read_web_settings, including the old upload-host migration.
    let value = raw
        .get("web_upload_url")
        .or_else(|| raw.get("toss_upload_url"));
    let value = match value {
        Some(Value::Text(value)) => value.as_str(),
        _ => "",
    };
    let default = openpilot_web_upload::DEFAULT_WEB_UPLOAD_URL;
    let mut url =
        openpilot_web_upload::normalize_base_url(value, default).unwrap_or_else(|_| default.into());
    if matches!(
        url.to_lowercase().as_str(),
        "https://op.wjcloud.kr" | "https://shind0.synology.me"
    ) {
        url = default.into();
    }
    [("web_upload_url".into(), Value::Text(url))]
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn owned_capture_argv_missing_provider_and_timeout_keep_metadata() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("tmux-provider");
        let mut native = Native {
            executable: PathBuf::new(),
            launcher: env::var_os("OPENPILOT_CRASH_CAPTURE_LAUNCHER")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    env::current_exe()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join("openpilot-process-child")
                }),
            tmux: path.clone(),
        };
        let event = store::fields(br#"{"pane":"owned:0.0"}"#).unwrap();
        assert_eq!(
            native.capture(&event).unwrap(),
            "Tmux capture unavailable: FileNotFoundError. Exit metadata retained.\n"
        );
        fs::write(&path, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            native.capture(&event).unwrap(),
            "capture-pane\n-p\n-J\n-S\n-1000\n-t\nowned:0.0\n"
        );
        fs::write(&path, "#!/bin/sh\nexit 7\n").unwrap();
        assert_eq!(
            native.capture(&event).unwrap(),
            "Tmux capture unavailable: CalledProcessError. Exit metadata retained.\n"
        );
        fs::write(&path, "#!/bin/sh\nexec sleep 30\n").unwrap();
        let started = std::time::Instant::now();
        assert_eq!(
            native.capture(&event).unwrap(),
            "Tmux capture unavailable: TimeoutExpired. Exit metadata retained.\n"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        // Upload helper cancellation is independent of a blocked provider and
        // reaps its exact direct child; no HTTP/provider is contacted here.
        native.executable = path;
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&stop);
        let task = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            cancel.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        assert!(!native
            .upload(
                "dsm",
                &Fields::new(),
                &root.path().join("report.txt"),
                &stop
            )
            .unwrap());
        assert!(started.elapsed() < Duration::from_secs(2));
        task.join().unwrap();
    }
}
