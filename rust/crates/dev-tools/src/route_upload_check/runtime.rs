//! Owned native collector/logger lifecycle for the composed runtime check.
use super::{
    binary_hashes, environment, http, lifecycle::Context, qlog, require, route_records,
    runtime_crash, spec, upload, Options,
};
use crate::qa_process::NativeRunner;
use crate::seal::Result;
use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    os::unix::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};
pub(super) struct HeldPath {
    pub(super) path: PathBuf,
    pub(super) held: File,
    pub(super) directory: bool,
    pub(super) receipt: bool,
    pub(super) preserve: bool,
}
impl HeldPath {
    pub(super) fn new(path: &Path, directory: bool) -> Result<Self> {
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
            receipt: false,
            preserve: false,
        })
    }
    pub(super) fn receipt(path: &Path, file: File) -> Result<Self> {
        require(file.metadata()?.is_file(), "receipt is not a regular file")?;
        Ok(Self {
            path: path.into(),
            held: file,
            directory: false,
            receipt: true,
            preserve: false,
        })
    }
    pub(super) fn matches_path(&self) -> Result<bool> {
        let expected = self.held.metadata()?;
        match fs::symlink_metadata(&self.path) {
            Ok(actual) => Ok((expected.dev(), expected.ino()) == (actual.dev(), actual.ino())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}
impl Drop for HeldPath {
    fn drop(&mut self) {
        if self.preserve {
            return;
        }
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
    pub(super) held: HeldPath,
}
impl Directory {
    pub(super) fn new(path: &Path) -> Result<Self> {
        fs::create_dir(path)?;
        Ok(Self {
            held: HeldPath::new(path, true)?,
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
        if self.exited()? {
            return Err("native service exited before readiness".into());
        }
        Ok(())
    }
    fn exited(&self) -> Result<bool> {
        let pid = Pid::from_child(&self.child);
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    fn stop(&mut self, context: &Context) -> Result<ExitStatus> {
        self.alive()?;
        let pid = Pid::from_raw(self.child.id() as i32).ok_or("invalid native child PID")?;
        kill_process_group(pid, Signal::TERM)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.exited()? {
                // The unreaped leader reserves the group identity until its
                // final signal, including descendants left by an exited leader.
                match kill_process_group(pid, Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => return Err(error.into()),
                }
                let status = self.child.wait()?;
                self.live = false;
                return Ok(status);
            }
            require(
                Instant::now() < deadline,
                "native service shutdown deadline",
            )?;
            context.sleep(Duration::from_millis(10))?;
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
struct Children(Vec<OwnedChild>);
impl Drop for Children {
    fn drop(&mut self) {
        while let Some(child) = self.0.pop() {
            drop(child);
        }
    }
}
fn wait_ready(
    mut condition: impl FnMut(&mut Context) -> Result<bool>,
    children: &mut [OwnedChild],
    context: &mut Context,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ready = condition(context)?;
        context.check()?;
        for child in children.iter_mut() {
            child.alive()?;
        }
        if ready {
            return Ok(());
        }
        require(
            Instant::now() < deadline,
            "native service readiness deadline",
        )?;
        // Re-enter the path-observation predicate before returning cancellation
        // so a partial collector bind is journalled before the children unwind.
        std::thread::sleep(Duration::from_millis(20));
    }
}
pub(super) fn run(options: &Options, context: &mut Context) -> Result<Value> {
    // pathlib.Path.mkdir(parents=True) has exist_ok=False in both originals.
    // Reject the complete destination before opening any diagnostic files.
    fs::create_dir_all(options.output.parent().ok_or("output parent missing")?)?;
    fs::create_dir(&options.output)?;
    let root = tempfile::Builder::new()
        .prefix("runtime-upload-")
        .tempdir()?;
    let root_identity = HeldPath::new(root.path(), true)?;
    let root = root.keep();
    context.own(&root_identity)?;
    let prefix = format!(
        "-runtime-upload-{}",
        root.as_path()
            .file_name()
            .ok_or("temporary root missing")?
            .to_str()
            .ok_or("temporary root name not UTF8")?
    );
    let namespace = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
    let _namespace = Directory::new(&namespace)?;
    context.own(&_namespace.held)?;
    let ipc_root = context.prepare_ipc()?;
    let socket = ipc_root.join("logmessage");
    require(
        fs::symlink_metadata(&socket)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "runtime log endpoint already exists",
    )?;
    let params = root.as_path().join("params").join(&prefix);
    fs::create_dir_all(&params)?;
    for (key, value) in [
        ("DongleId", "0000000000000000"),
        ("IsOffroad", "1"),
        ("GitCommit", "owned-runtime-upload"),
    ] {
        fs::write(params.join(key), value)?;
    }
    fs::write(
        root.as_path().join("build.json"),
        br#"{"openpilot":{"git_origin":"fixture/owned"}}"#,
    )?;
    let routes = options.output.join("routes");
    fs::create_dir(&routes)?;
    let mut env = environment();
    env.insert("ROUTE_UPLOAD_QA_IPC_ROOT".into(), ipc_root.into_os_string());
    for (key, value) in [
        ("HOME", root.as_path().as_os_str().to_owned()),
        ("OPENPILOT_PREFIX", prefix.clone().into()),
        (
            "PARAMS_ROOT",
            root.as_path().join("params").into_os_string(),
        ),
        ("LOG_ROOT", routes.clone().into_os_string()),
        ("OPENPILOT_BASEDIR", root.as_path().as_os_str().to_owned()),
        ("OPENPILOT_ROOT", root.as_path().as_os_str().to_owned()),
        ("RUST_BACKTRACE", "1".into()),
    ] {
        env.insert(key.into(), value);
    }
    let swaglog = options.output.join("swaglog");
    // This guard precedes the children: children are stopped/reaped before its
    // held endpoint identity is removed on any readiness/failure unwind.
    let mut socket_identity = None;
    let mut children = Children(vec![OwnedChild::start(
        &options.binaries.join("openpilot-logmessaged"),
        &["--log-root".into(), swaglog.into_os_string()],
        &env,
        &options.output.join("openpilot-logmessaged.log"),
    )?]);
    wait_ready(
        |context| {
            if socket_identity.is_none() && socket.exists() {
                let mut identity = HeldPath::new(&socket, false)?;
                identity.preserve = true;
                context.own(&identity)?;
                identity.preserve = false;
                socket_identity = Some(identity);
            }
            Ok(socket_identity.is_some() && namespace.join("errorLogMessage").exists())
        },
        &mut children.0,
        context,
    )?;
    children.0.push(OwnedChild::start(
        &options.binaries.join("openpilot-loggerd"),
        &[],
        &env,
        &options.output.join("openpilot-loggerd.log"),
    )?);
    wait_ready(
        |_| Ok(params.join("CurrentRoute").exists()),
        &mut children.0,
        context,
    )?;
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
    let failure = NativeRunner.run_controlled(
        &command,
        Instant::now() + command.timeout,
        &context.cancel.flag,
    );
    context.check()?;
    let failure = failure?;
    fs::write(
        &failure_log,
        [&failure.stdout[..], &failure.stderr[..]].concat(),
    )?;
    require(
        failure.code == 1 && failure.stderr.windows(6).any(|bytes| bytes == b"crash\n"),
        "managed uploader failure did not report crash",
    )?;
    context.sleep(Duration::from_millis(300))?;
    require(
        children.0[1].stop(context)?.code() == Some(0),
        "route logger shutdown failed",
    )?;
    require(
        children.0[0].stop(context)?.code() == Some(0),
        "collector shutdown failed",
    )?;
    let qlog = qlog(&routes)?;
    http::signing_key(
        &root
            .as_path()
            .join(format!(".comma{prefix}"))
            .join("persist"),
    )?;
    let transfers = upload(options, &env, "1", 15, &qlog, &routes, context)?;
    require(
        transfers.len() == 1,
        "expected one qlog request and transfer",
    )?;
    let bytes = fs::read(&qlog)?;
    let (messages, errors) = route_records(&bytes, "errorLogMessage")?;
    let crash = runtime_crash(&errors)?;
    let receipt = json!({"result":"PASS","upload_bytes":bytes.len(),"uploaded_sha256":super::hash(&bytes),"route_messages":messages,"typed_crash_records":1,"upload_marked":true,"source_commit":crash["ctx"]["source_commit"],"source_tree":crash["ctx"]["source_tree"],"binaries":binary_hashes(&options.binaries,&["openpilot-logmessaged","openpilot-loggerd","openpilot-uploader"] )?,"scope":"real native child, collector, route logger and uploader with owned storage and HTTP; no complete manager or device claim"});
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
