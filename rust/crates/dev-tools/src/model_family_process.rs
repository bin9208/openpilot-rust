//! Small process/namespace owner shared by the model startup and USB loop checks.
use crate::seal::Result;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, kill_process_group, Pid, Signal},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    os::unix::{fs::FileTypeExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const LOG_LIMIT: usize = 16 * 1024 * 1024;
pub(crate) struct Cancellation {
    flag: Arc<AtomicBool>,
    signals: Vec<signal_hook::SigId>,
}
impl Cancellation {
    pub fn new() -> Result<Self> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut result = Self {
            flag,
            signals: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            result
                .signals
                .push(signal_hook::flag::register(signal, result.flag.clone())?);
        }
        Ok(result)
    }
    pub fn check(&self) -> Result<()> {
        if self.flag.load(Ordering::Relaxed) {
            Err("model family check cancelled".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Cancellation {
    fn drop(&mut self) {
        for id in self.signals.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub(crate) struct Process {
    child: Child,
    stream: File,
    log: File,
    pending: Vec<u8>,
    bytes: usize,
    status: Option<ExitStatus>,
    argv: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
    removed: Vec<String>,
    group_cleaned: bool,
}
impl Process {
    pub fn spawn(
        binary: &Path,
        args: &[OsString],
        environment: &BTreeMap<OsString, OsString>,
        remove: &[&str],
        log: &Path,
    ) -> Result<Self> {
        let mut command = Command::new(binary);
        command
            .args(args)
            .envs(environment)
            .stdin(Stdio::piped())
            .process_group(0);
        for name in remove {
            command.env_remove(name);
        }
        // Merge both streams into one bounded reader. No undrained pipe or
        // unbounded auxiliary output file can fill the runner disk.
        let (reader, writer) = std::os::unix::net::UnixStream::pair()?;
        command
            .stdout(Stdio::from(std::os::fd::OwnedFd::from(writer.try_clone()?)))
            .stderr(Stdio::from(std::os::fd::OwnedFd::from(writer)));
        let log = File::options().write(true).create_new(true).open(log)?;
        let child = command.spawn()?;
        let stream = File::from(std::os::fd::OwnedFd::from(reader));
        let mut result = Self {
            child,
            stream,
            log,
            pending: Vec::new(),
            bytes: 0,
            status: None,
            argv: std::iter::once(binary.as_os_str().to_owned())
                .chain(args.iter().cloned())
                .collect(),
            environment: environment.clone(),
            removed: remove.iter().map(|s| s.to_string()).collect(),
            group_cleaned: false,
        };
        let flags = fcntl_getfl(&result.stream)?;
        fcntl_setfl(&result.stream, flags | OFlags::NONBLOCK)?;
        result.drain()?;
        Ok(result)
    }
    fn drain(&mut self) -> Result<bool> {
        let mut eof = false;
        loop {
            let mut bytes = [0; 8192];
            match self.stream.read(&mut bytes) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => {
                    self.bytes += n;
                    if self.bytes > LOG_LIMIT {
                        return Err("model check child log exceeds bound".into());
                    }
                    self.log.write_all(&bytes[..n])?;
                    self.pending.extend_from_slice(&bytes[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(eof)
    }
    pub fn line(
        &mut self,
        wanted: &[&str],
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<Option<String>> {
        let deadline = Instant::now() + timeout;
        loop {
            cancel.check()?;
            while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
                let line = self.pending.drain(..=end).collect::<Vec<_>>();
                let line = String::from_utf8_lossy(&line)
                    .trim_end_matches('\n')
                    .to_owned();
                if wanted.iter().any(|s| line.contains(s)) {
                    return Ok(Some(line));
                }
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            let duration = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            let timeout = Timespec::try_from(duration)?;
            let mut pollfd = [PollFd::new(&self.stream, PollFlags::IN)];
            match poll(&mut pollfd, Some(&timeout)) {
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
                Ok(_) => {
                    let eof = self.drain()?;
                    if eof && !self.pending.is_empty() && !self.pending.contains(&b'\n') {
                        return Err("model check child closed a partial line before newline".into());
                    }
                    if eof && self.pending.is_empty() {
                        return Err(
                            "model check child closed its output before expected line".into()
                        );
                    }
                }
            }
        }
    }
    pub fn require_line(
        &mut self,
        wanted: &[&str],
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<String> {
        self.line(wanted, timeout, cancel)?
            .ok_or_else(|| "model check child line deadline exceeded".into())
    }
    pub fn command(&mut self, text: &str, cancel: &Cancellation) -> Result<String> {
        let input = self
            .child
            .stdin
            .as_mut()
            .ok_or("model peer stdin missing")?;
        writeln!(input, "{text}")?;
        input.flush()?;
        self.require_line(&["OK", "TIMEOUT", "ERR"], Duration::from_secs(60), cancel)
    }
    pub fn receipt(&self) -> serde_json::Value {
        use std::os::unix::{ffi::OsStrExt, process::ExitStatusExt};
        serde_json::json!({"argv_bytes":self.argv.iter().map(|a|a.as_bytes().to_vec()).collect::<Vec<_>>(),"environment_overlay_bytes":self.environment.iter().map(|(k,v)|(k.as_bytes().to_vec(),v.as_bytes().to_vec())).collect::<Vec<_>>(),"removed_environment":self.removed,"exit_code":self.status.as_ref().and_then(ExitStatus::code),"signal":self.status.as_ref().and_then(ExitStatusExt::signal),"captured_log_bytes":self.bytes,"merged_output":true})
    }
    pub fn alive(&mut self) -> Result<bool> {
        if self.status.is_none() {
            self.status = self.child.try_wait()?;
        }
        Ok(self.status.is_none())
    }
    pub fn signal(&self, signal: Signal) -> Result<()> {
        if self.status.is_none() {
            kill_process(
                Pid::from_raw(self.child.id() as i32).ok_or("model process PID missing")?,
                signal,
            )?;
        }
        Ok(())
    }
    pub fn wait(&mut self, timeout: Duration, cancel: &Cancellation) -> Result<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            cancel.check()?;
            self.drain()?;
            if !self.alive()? {
                return self
                    .status
                    .ok_or_else(|| "model process status missing".into());
            }
            if Instant::now() >= deadline {
                return Err("model check child exit deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn stop(&mut self, cancel: &Cancellation) -> Result<ExitStatus> {
        if self.status.is_none() {
            self.signal(Signal::Term)?;
        }
        let status = self.wait(Duration::from_secs(5), cancel)?;
        self.cleanup_group();
        Ok(status)
    }
    fn cleanup_group(&mut self) {
        // The child starts an exclusive process group. Its leader may already
        // have exited while a worker still holds that group alive.
        if self.group_cleaned {
            return;
        }
        self.group_cleaned = true;
        if let Some(pid) = Pid::from_raw(self.child.id() as i32) {
            let _ = kill_process_group(pid, Signal::Kill);
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.status.is_none() {
            // Give the daemon its original five-second SIGTERM cleanup window
            // so its separately owned USB worker/shared memory can be reaped.
            let _ = self.signal(Signal::Term);
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                let _ = self.drain();
                match self.child.try_wait() {
                    Ok(Some(status)) => {
                        self.status = Some(status);
                        break;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                    Err(_) => break,
                }
            }
        }
        self.cleanup_group();
        if self.status.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = self.drain();
    }
}
pub(crate) struct Namespace {
    _directory: tempfile::TempDir,
    prefix: String,
}
impl Namespace {
    pub fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("msgq_model-family-")
            .tempdir_in("/dev/shm")?;
        let prefix = directory
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("msgq_"))
            .ok_or("model namespace name invalid")?
            .to_owned();
        Ok(Self {
            _directory: directory,
            prefix,
        })
    }
    pub fn environment(&self, params: &Path) -> BTreeMap<OsString, OsString> {
        BTreeMap::from([
            ("OPENPILOT_PREFIX".into(), self.prefix.clone().into()),
            ("PARAMS_ROOT".into(), params.as_os_str().to_owned()),
            ("LOGPRINT".into(), "info".into()),
        ])
    }
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    pub fn finish(&self) -> Result<()> {
        for entry in fs::read_dir("/tmp")? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&format!("{}_visionipc_", self.prefix)))
            {
                if !entry.file_type()?.is_socket() {
                    return Err("foreign model namespace socket replacement preserved".into());
                }
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}
impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
pub(crate) fn protocol_path(path: &Path) -> Result<&str> {
    let value = path
        .to_str()
        .ok_or("model peer protocol path is not UTF-8")?;
    if value.chars().any(char::is_whitespace) {
        return Err("model peer protocol path contains whitespace".into());
    }
    Ok(value)
}
pub(crate) fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
pub(crate) fn layout(width: u64) -> Result<(u64, u64, u64, u64, u64)> {
    let height: u64 = match width {
        1344 => 760,
        1928 => 1208,
        _ => return Err("unsupported original camera resolution".into()),
    };
    let align = |n: u64, a: u64| n.div_ceil(a) * a;
    let stride = align(width, 128);
    let y_height = align(height, 32);
    let uv_height = align(height / 2, 16);
    let size = align(
        align(
            stride * y_height + stride * uv_height + 4096 + (8 * stride).max(16 * 1024),
            4096,
        ) + align(width, 512) * 512,
        4096,
    );
    Ok((height, stride, y_height, stride * y_height, size))
}
pub(crate) fn car_params(path: &Path, brand: bool) -> Result<()> {
    let mut message = capnp::message::Builder::new_default();
    let mut cp = message.init_root::<openpilot_cereal::car_capnp::car_params::Builder<'_>>();
    cp.set_longitudinal_actuator_delay(0.0);
    if brand {
        cp.set_brand("owned");
    }
    fs::write(path, capnp::serialize::write_message_to_words(&message))?;
    Ok(())
}

pub(crate) struct Held {
    path: PathBuf,
    file: File,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    digest: String,
}
impl Held {
    fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
        use std::os::unix::fs::MetadataExt;
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    }
    fn hash(file: &File) -> Result<String> {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::FileExt;
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut bytes = [0; 65536];
        let length = file.metadata()?.len();
        require(
            length <= 800 * 1024 * 1024,
            "model family input grew past hash bound",
        )?;
        while offset < length {
            let count = ((length - offset) as usize).min(bytes.len());
            let n = file.read_at(&mut bytes[..count], offset)?;
            if n == 0 {
                break;
            }
            hash.update(&bytes[..n]);
            offset += n as u64;
        }
        require(
            offset == length,
            "model family held input was truncated while hashing",
        )?;
        Ok(format!("{:x}", hash.finalize()))
    }
    pub fn open(path: &Path, max: u64, elf: bool) -> Result<Self> {
        for parent in path.ancestors().skip(1) {
            require(
                fs::symlink_metadata(parent)?.is_dir(),
                "model family input parent alias or missing directory",
            )?;
        }
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let file = File::from(fd);
        let m = file.metadata()?;
        require(
            m.is_file() && m.len() <= max,
            "model family held input is not bounded regular data",
        )?;
        if elf {
            use std::os::unix::fs::FileExt;
            let mut header = [0; 20];
            file.read_exact_at(&mut header, 0)?;
            let arch = if cfg!(target_arch = "aarch64") {
                183
            } else {
                62
            };
            require(
                &header[..6] == b"\x7fELF\x02\x01"
                    && u16::from_le_bytes([header[18], header[19]]) == arch,
                "model family binary is not a current architecture ELF",
            )?;
        }
        let result = Self {
            path: path.to_owned(),
            identity: Self::identity(&m),
            digest: Self::hash(&file)?,
            file,
        };
        result.verify()?;
        Ok(result)
    }
    pub fn verify(&self) -> Result<()> {
        for parent in self.path.ancestors().skip(1) {
            require(
                fs::symlink_metadata(parent)?.is_dir(),
                "model family input parent changed to alias",
            )?;
        }
        let m = fs::symlink_metadata(&self.path)?;
        require(
            m.is_file()
                && !m.file_type().is_symlink()
                && Self::identity(&m) == self.identity
                && Self::identity(&self.file.metadata()?) == self.identity
                && Self::hash(&self.file)? == self.digest,
            "model family held input changed",
        )
    }
    pub fn bytes(&self, max: u64) -> Result<Vec<u8>> {
        use std::os::unix::fs::FileExt;
        require(
            self.identity.2 <= max,
            "model family data exceeds read bound",
        )?;
        let mut bytes = vec![0; self.identity.2 as usize];
        self.file.read_exact_at(&mut bytes, 0)?;
        self.verify()?;
        Ok(bytes)
    }
    pub fn pin(&self) -> serde_json::Value {
        serde_json::json!({"bytes":self.identity.2,"sha256":self.digest})
    }
    pub fn expect(&self, seal: &serde_json::Value) -> Result<()> {
        require(
            self.pin() == *seal,
            "model family input differs from authenticated original seal",
        )
    }
}
pub(crate) fn relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    require(
        !name.is_empty()
            && name
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != "..")
            && path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "model family original path escapes fixture",
    )?;
    Ok(path)
}
pub(crate) fn held_tree(root: &Path) -> Result<Vec<Held>> {
    fn visit(path: &Path, output: &mut Vec<Held>, total: &mut u64) -> Result<()> {
        let m = fs::symlink_metadata(path)?;
        require(
            !m.file_type().is_symlink(),
            "model family tree contains alias",
        )?;
        if m.is_dir() {
            for entry in fs::read_dir(path)? {
                visit(&entry?.path(), output, total)?;
            }
        } else {
            *total = total
                .checked_add(m.len())
                .ok_or("model family tree size overflow")?;
            require(
                output.len() < 2048 && *total <= 512 * 1024 * 1024,
                "model family tree exceeds bounds",
            )?;
            output.push(Held::open(path, 512 * 1024 * 1024, false)?);
        }
        Ok(())
    }
    let mut files = Vec::new();
    let mut total = 0;
    visit(root, &mut files, &mut total)?;
    Ok(files)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn venus_layout_matches_both_original_camera_shapes() {
        assert_eq!(layout(1344).unwrap(), (760, 1408, 768, 1081344, 2428928));
        assert_eq!(layout(1928).unwrap(), (1208, 2048, 1216, 2490368, 4804608));
        assert!(layout(0).is_err());
    }
    #[test]
    fn held_source_detects_replacement_and_traversal() {
        let root = tempfile::tempdir().unwrap();
        let p = root.path().join("source");
        fs::write(&p, b"old").unwrap();
        let held = Held::open(&p, 3, false).unwrap();
        fs::remove_file(&p).unwrap();
        fs::write(&p, b"old").unwrap();
        assert!(held.verify().is_err());
        for name in ["../escape", "/absolute", "./dot", "a//b"] {
            assert!(relative(name).is_err());
        }
    }
}
pub(crate) fn verify_tree(root: &Path, held: &[Held]) -> Result<()> {
    fn names(path: &Path, out: &mut std::collections::BTreeSet<PathBuf>) -> Result<()> {
        let m = fs::symlink_metadata(path)?;
        require(
            !m.file_type().is_symlink(),
            "model family tree alias reappeared",
        )?;
        if m.is_dir() {
            for entry in fs::read_dir(path)? {
                names(&entry?.path(), out)?;
            }
        } else {
            require(
                m.is_file() && out.len() < 2048,
                "model family tree changed type or exceeded bound",
            )?;
            out.insert(path.to_owned());
        }
        Ok(())
    }
    let mut current = std::collections::BTreeSet::new();
    names(root, &mut current)?;
    require(
        current == held.iter().map(|p| p.path.clone()).collect(),
        "model family tree whole roster changed",
    )?;
    for file in held {
        file.verify()?;
    }
    Ok(())
}
pub(crate) fn publish_report(
    path: &Path,
    value: &serde_json::Value,
    mut verify: impl FnMut() -> Result<()>,
) -> Result<()> {
    struct Owned {
        path: PathBuf,
        file: File,
        accepted: bool,
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            use std::os::unix::fs::MetadataExt;
            if !self.accepted {
                if let (Ok(a), Ok(b)) = (self.file.metadata(), fs::symlink_metadata(&self.path)) {
                    if b.is_file()
                        && !b.file_type().is_symlink()
                        && a.dev() == b.dev()
                        && a.ino() == b.ino()
                    {
                        let _ = fs::remove_file(&self.path);
                    }
                }
            }
        }
    }
    verify()?;
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut owned = Owned {
        path: path.to_owned(),
        file,
        accepted: false,
    };
    let mut expected = serde_json::to_vec_pretty(value)?;
    expected.push(b'\n');
    owned.file.write_all(&expected)?;
    owned.file.flush()?;
    verify()?;
    use std::os::unix::fs::{FileExt, MetadataExt};
    let original = owned.file.metadata()?;
    let current = fs::symlink_metadata(path)?;
    require(
        current.is_file()
            && !current.file_type().is_symlink()
            && original.dev() == current.dev()
            && original.ino() == current.ino()
            && current.len() == expected.len() as u64,
        "model family success report was replaced",
    )?;
    let mut actual = vec![0; expected.len()];
    owned.file.read_exact_at(&mut actual, 0)?;
    require(
        actual == expected,
        "model family success report contents changed",
    )?;
    owned.accepted = true;
    Ok(())
}

pub(crate) fn json_file(path: &Path, max: u64) -> Result<serde_json::Value> {
    let held = Held::open(path, max, false)?;
    Ok(serde_json::from_slice(&held.bytes(max)?)?)
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn successful_final_verifier_cannot_accept_foreign_or_modified_report() {
        let root = tempfile::tempdir().unwrap();
        for replace in [false, true] {
            let report = root.path().join(if replace {
                "foreign.json"
            } else {
                "modified.json"
            });
            let mut calls = 0;
            assert!(
                publish_report(&report, &serde_json::json!({"passed":true}), || {
                    calls += 1;
                    if calls == 2 {
                        if replace {
                            fs::remove_file(&report)?;
                        }
                        fs::write(&report, b"foreign")?;
                    }
                    Ok(())
                })
                .is_err()
            );
            if replace {
                assert_eq!(fs::read(report).unwrap(), b"foreign");
            } else {
                assert!(!report.exists());
            }
        }
    }
    #[test]
    fn stop_cleans_worker_group_after_leader_was_reaped() {
        let root = tempfile::tempdir().unwrap();
        let cancel = Cancellation::new().unwrap();
        let mut child = Process::spawn(
            Path::new("/bin/sh"),
            &["-c".into(), "sleep 60 & echo READY:$!; exit 0".into()],
            &BTreeMap::new(),
            &[],
            &root.path().join("child.log"),
        )
        .unwrap();
        let line = child
            .require_line(&["READY:"], Duration::from_secs(2), &cancel)
            .unwrap();
        let worker: i32 = line.strip_prefix("READY:").unwrap().trim().parse().unwrap();
        assert!(child
            .wait(Duration::from_secs(2), &cancel)
            .unwrap()
            .success());
        assert!(child.stop(&cancel).unwrap().success());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state = fs::read_to_string(format!("/proc/{worker}/stat"));
            if state
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                || state
                    .as_ref()
                    .is_ok_and(|s| s.rsplit_once(") ").is_some_and(|(_, s)| s.starts_with('Z')))
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "worker remained alive after leader exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn merged_peer_reply_is_not_lost_when_exit_and_eof_arrive_together() {
        let root = tempfile::tempdir().unwrap();
        let cancel = Cancellation::new().unwrap();
        let mut child = Process::spawn(
            Path::new("/bin/sh"),
            &[
                "-c".into(),
                "printf 'READY\\n'; read command; printf 'OK\\n'; exit 0".into(),
            ],
            &BTreeMap::new(),
            &[],
            &root.path().join("child.log"),
        )
        .unwrap();
        child
            .require_line(&["READY"], Duration::from_secs(2), &cancel)
            .unwrap();
        assert_eq!(child.command("stop", &cancel).unwrap(), "OK");
        assert!(child
            .wait(Duration::from_secs(2), &cancel)
            .unwrap()
            .success());
        assert_eq!(child.receipt()["exit_code"], 0);
    }
    #[test]
    fn late_input_failure_rolls_back_owned_report_but_preserves_foreign_replacement() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::write(&source, b"held").unwrap();
        let held = Held::open(&source, 4, false).unwrap();
        let report = root.path().join("report.json");
        let mut calls = 0;
        assert!(
            publish_report(&report, &serde_json::json!({"result":"pass"}), || {
                calls += 1;
                if calls == 2 {
                    assert!(report.exists());
                    fs::write(&source, b"late")?;
                }
                held.verify()
            })
            .is_err()
        );
        assert!(!report.exists());
        let mut calls = 0;
        assert!(
            publish_report(&report, &serde_json::json!({"result":"pass"}), || {
                calls += 1;
                if calls == 2 {
                    fs::remove_file(&report)?;
                    fs::write(&report, b"foreign")?;
                    return Err("cancelled".into());
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(report).unwrap(), b"foreign");
    }
}

// Failure diagnostics are not success receipts and never overwrite a foreign file.
pub(crate) struct FailedCase {
    path: PathBuf,
    value: serde_json::Value,
    active: bool,
}
impl FailedCase {
    pub fn new(path: PathBuf, value: serde_json::Value) -> Self {
        Self {
            path,
            value,
            active: true,
        }
    }
    pub fn observed(&mut self, value: serde_json::Value) {
        self.value = value;
    }
    pub fn accepted(&mut self) {
        self.active = false;
    }
}
impl Drop for FailedCase {
    fn drop(&mut self) {
        if self.active {
            if let Ok(mut file) = File::options()
                .write(true)
                .create_new(true)
                .open(&self.path)
            {
                if let Ok(bytes) = serde_json::to_vec_pretty(&self.value) {
                    let _ = file.write_all(&bytes);
                    let _ = file.write_all(b"\n");
                    let _ = file.flush();
                }
            }
        }
    }
}
