//! Owned native collector/logger lifecycle for the composed runtime check.
use super::{
    binary_hashes, environment, http, qlog, require, route_records, runtime_crash, spec, upload,
    write_json, Options,
};
use crate::qa_process::{NativeRunner, Runner};
use crate::seal::Result;
use rustix::process::{kill_process_group, Pid, Signal};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    os::unix::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
struct HeldPath {
    path: PathBuf,
    held: File,
    directory: bool,
}
impl HeldPath {
    fn new(path: &Path, directory: bool) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let held = File::from(fd);
        require(
            if directory {
                held.metadata()?.is_dir()
            } else {
                use std::os::unix::fs::FileTypeExt;
                held.metadata()?.file_type().is_socket()
            },
            "owned runtime path type changed",
        )?;
        Ok(Self {
            path: path.into(),
            held,
            directory,
        })
    }
}
impl Drop for HeldPath {
    fn drop(&mut self) {
        if let (Ok(expected), Ok(actual)) = (self.held.metadata(), fs::symlink_metadata(&self.path))
        {
            if (expected.dev(), expected.ino()) == (actual.dev(), actual.ino()) {
                if self.directory {
                    let _ = fs::remove_dir_all(&self.path);
                } else {
                    let _ = fs::remove_file(&self.path);
                }
            }
        }
    }
}
pub(super) struct Directory {
    _held: HeldPath,
}
impl Directory {
    pub(super) fn new(path: &Path) -> Result<Self> {
        fs::create_dir(path)?;
        Ok(Self {
            _held: HeldPath::new(path, true)?,
        })
    }
}
struct OwnedChild {
    child: Child,
    live: bool,
}
impl OwnedChild {
    fn start(
        binary: &Path,
        arguments: &[std::ffi::OsString],
        env: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
        log: &Path,
    ) -> Result<Self> {
        let output = File::create(log)?;
        let child = Command::new(binary)
            .args(arguments)
            .env_clear()
            .envs(env)
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output)
            .process_group(0)
            .spawn()?;
        Ok(Self { child, live: true })
    }
    fn alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            self.live = false;
            return Err(format!("native service exited before readiness: {status}").into());
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<ExitStatus> {
        self.alive()?;
        let pid = Pid::from_raw(self.child.id() as i32).ok_or("invalid native child PID")?;
        kill_process_group(pid, Signal::TERM)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.live = false;
                return Ok(status);
            }
            require(
                Instant::now() < deadline,
                "native service shutdown deadline",
            )?;
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.live {
            if let Some(pid) = Pid::from_raw(self.child.id() as i32) {
                let _ = kill_process_group(pid, Signal::KILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn wait_ready(mut condition: impl FnMut() -> bool, children: &mut [OwnedChild]) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        for child in children.iter_mut() {
            child.alive()?;
        }
        require(
            Instant::now() < deadline,
            "native service readiness deadline",
        )?;
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
pub(super) fn run(options: &Options) -> Result<Value> {
    // pathlib.Path.mkdir(parents=True) has exist_ok=False in both originals.
    // Reject the complete destination before opening any diagnostic files.
    fs::create_dir_all(options.output.parent().ok_or("output parent missing")?)?;
    fs::create_dir(&options.output)?;
    let root = tempfile::Builder::new()
        .prefix("runtime-upload-")
        .tempdir()?;
    let prefix = format!(
        "-runtime-upload-{}",
        root.path()
            .file_name()
            .ok_or("temporary root missing")?
            .to_str()
            .ok_or("temporary root name not UTF8")?
    );
    let namespace = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
    let _namespace = Directory::new(&namespace)?;
    let socket = Path::new("/tmp").join(format!("logmessage{prefix}"));
    require(
        fs::symlink_metadata(&socket)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "runtime log endpoint already exists",
    )?;
    let params = root.path().join("params").join(&prefix);
    fs::create_dir_all(&params)?;
    for (key, value) in [
        ("DongleId", "0000000000000000"),
        ("IsOffroad", "1"),
        ("GitCommit", "owned-runtime-upload"),
    ] {
        fs::write(params.join(key), value)?;
    }
    fs::write(
        root.path().join("build.json"),
        br#"{"openpilot":{"git_origin":"fixture/owned"}}"#,
    )?;
    let routes = options.output.join("routes");
    fs::create_dir(&routes)?;
    let mut env = environment();
    for (key, value) in [
        ("HOME", root.path().as_os_str().to_owned()),
        ("OPENPILOT_PREFIX", prefix.clone().into()),
        ("PARAMS_ROOT", root.path().join("params").into_os_string()),
        ("LOG_ROOT", routes.clone().into_os_string()),
        ("OPENPILOT_BASEDIR", root.path().as_os_str().to_owned()),
        ("OPENPILOT_ROOT", root.path().as_os_str().to_owned()),
        ("RUST_BACKTRACE", "1".into()),
    ] {
        env.insert(key.into(), value);
    }
    let swaglog = options.output.join("swaglog");
    let mut children = vec![OwnedChild::start(
        &options.binaries.join("openpilot-logmessaged"),
        &["--log-root".into(), swaglog.into_os_string()],
        &env,
        &options.output.join("openpilot-logmessaged.log"),
    )?];
    wait_ready(
        || socket.exists() && namespace.join("errorLogMessage").exists(),
        &mut children,
    )?;
    let _socket = HeldPath::new(&socket, false)?;
    children.push(OwnedChild::start(
        &options.binaries.join("openpilot-loggerd"),
        &[],
        &env,
        &options.output.join("openpilot-loggerd.log"),
    )?);
    wait_ready(|| params.join("CurrentRoute").exists(), &mut children)?;
    let mut failure_env = env.clone();
    failure_env.insert("MANAGER_DAEMON".into(), "uploader".into());
    failure_env.insert(
        "OPENPILOT_LOG_CONTEXT".into(),
        r#"{"dongle_id":"owned-device","version":"owned-version"}"#.into(),
    );
    let mut command = spec(
        vec![
            options.binaries.join("openpilot-uploader").into_os_string(),
            "--invalid-managed-fixture".into(),
        ],
        &failure_env,
        10,
    );
    command
        .env_remove
        .retain(|key| key != "MANAGER_DAEMON" && key != "OPENPILOT_LOG_CONTEXT");
    let failure_log = options.output.join("managed-failure.log");
    let failure = NativeRunner.run(&command)?;
    fs::write(
        &failure_log,
        [&failure.stdout[..], &failure.stderr[..]].concat(),
    )?;
    require(
        failure.code == 1 && failure.stderr.windows(6).any(|bytes| bytes == b"crash\n"),
        "managed uploader failure did not report crash",
    )?;
    thread::sleep(Duration::from_millis(300));
    require(
        children[1].stop()?.code() == Some(0),
        "route logger shutdown failed",
    )?;
    require(
        children[0].stop()?.code() == Some(0),
        "collector shutdown failed",
    )?;
    let qlog = qlog(&routes)?;
    http::signing_key(&root.path().join(format!(".comma{prefix}")).join("persist"))?;
    let transfers = upload(options, &env, "1", 15, &options.output, &qlog, &routes)?;
    require(
        transfers.len() == 1,
        "expected one qlog request and transfer",
    )?;
    let bytes = fs::read(&qlog)?;
    let (messages, errors) = route_records(&bytes, "errorLogMessage")?;
    let crash = runtime_crash(&errors)?;
    let receipt = json!({"result":"PASS","upload_bytes":bytes.len(),"uploaded_sha256":super::hash(&bytes),"route_messages":messages,"typed_crash_records":1,"upload_marked":true,"source_commit":crash["ctx"]["source_commit"],"source_tree":crash["ctx"]["source_tree"],"binaries":binary_hashes(&options.binaries,&["openpilot-logmessaged","openpilot-loggerd","openpilot-uploader"] )?,"scope":"real native child, collector, route logger and uploader with owned storage and HTTP; no complete manager or device claim"});
    write_json(&options.output.join("result.json"), &receipt)?;
    Ok(receipt)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_cleanup_preserves_replacement_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("owned");
        let owned = Directory::new(&path).unwrap();
        assert!(Directory::new(&path).is_err());
        fs::rename(&path, temporary.path().join("old")).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("foreign"), b"keep").unwrap();
        drop(owned);
        assert_eq!(fs::read(path.join("foreign")).unwrap(), b"keep");
    }
}
