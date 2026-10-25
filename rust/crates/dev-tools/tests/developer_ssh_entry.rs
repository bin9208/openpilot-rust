//! Actions 전용 actual entry 경계. frozen39/원문/production은 수정하지 않는다.
#![cfg(all(feature = "developer-ssh-native", target_os = "linux"))]

use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    ffi::OsString,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, FileExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAGIC: &[u8] = b"DSSHENTRY1\0";
const FILE_CAP: u64 = 128 * 1024 * 1024;
const MARKER_CAP: u64 = 64 * 1024;
const PREFIX: &str = "_entry523";
static COHORT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn env_path(name: &str) -> Result<PathBuf> {
    Ok(std::env::var_os(name)
        .ok_or_else(|| format!("required {name}"))?
        .into())
}
fn create(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}
fn mkdir(path: &Path) -> Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)?;
    Ok(())
}

#[test]
fn entry_held_files_reject_rewrite_replacement_and_new_alias() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("held");
    create(&path, b"ABCD", 0o600).unwrap();
    let held = Held::open(path.clone(), 4).unwrap();
    held.verify().unwrap();
    fs::write(&path, b"WXYZ").unwrap();
    // timestamp 정밀도와 관계없이 같은 길이 rewrite를 거부하고 FD hash도 바뀐다.
    let error = held.verify().unwrap_err().to_string();
    assert!(matches!(
        error.as_str(),
        "held identity changed" | "held bytes changed"
    ));
    assert_eq!(Held::hash(&held.file, 4).unwrap(), sha(b"WXYZ"));
    fs::remove_file(&path).unwrap();
    create(&path, b"ABCD", 0o600).unwrap();
    assert_eq!(
        held.verify().unwrap_err().to_string(),
        "held identity changed"
    );
    let held = Held::open(path.clone(), 4).unwrap();
    fs::hard_link(&path, temp.path().join("alias")).unwrap();
    assert_eq!(
        held.verify().unwrap_err().to_string(),
        "held identity changed"
    );
    let link = temp.path().join("symlink");
    symlink(&path, &link).unwrap();
    assert_eq!(
        Held::open(link, 4).err().unwrap().to_string(),
        "held path is not canonical"
    );
}

#[test]
fn entry_recorder_rejects_bad_nonce_truncation_trailing_and_marker_roster() {
    let nonce = "0".repeat(32);
    let mut bytes = MAGIC.to_vec();
    for number in [10_u32, 9, 8, 7] {
        bytes.extend(number.to_le_bytes());
    }
    bytes.extend(1_u64.to_le_bytes());
    for blob in [nonce.as_bytes(), b"/private/ssh", b"/private"] {
        bytes.extend((blob.len() as u32).to_le_bytes());
        bytes.extend(blob);
    }
    bytes.extend(1_u32.to_le_bytes());
    bytes.extend(3_u32.to_le_bytes());
    bytes.extend(b"ssh");
    Record::decode(&bytes, &nonce).unwrap();
    assert_eq!(
        Record::decode(&bytes, &"1".repeat(32))
            .unwrap_err()
            .to_string(),
        "recorder nonce differs"
    );
    assert_eq!(
        Record::decode(&bytes[..bytes.len() - 1], &nonce)
            .unwrap_err()
            .to_string(),
        "truncated recorder"
    );
    let mut extra = bytes.clone();
    extra.push(0);
    assert_eq!(
        Record::decode(&extra, &nonce).unwrap_err().to_string(),
        "recorder has trailing data"
    );
    let temp = tempfile::tempdir().unwrap();
    assert!(marker(temp.path(), false, &nonce).unwrap().is_none());
    create(&temp.path().join("exec.bin"), &bytes, 0o600).unwrap();
    marker(temp.path(), true, &nonce).unwrap();
    create(&temp.path().join("extra"), b"extra", 0o600).unwrap();
    assert_eq!(
        marker(temp.path(), true, &nonce).unwrap_err().to_string(),
        "recorder marker roster differs"
    );
    fs::remove_file(temp.path().join("extra")).unwrap();
    fs::remove_file(temp.path().join("exec.bin")).unwrap();
    std::os::unix::fs::symlink("absent", temp.path().join("exec.bin")).unwrap();
    assert!(marker(temp.path(), true, &nonce).is_err());
}

#[test]
#[ignore = "requires actual hosted SDK-compiled recorder and private proof inputs"]
fn actual_native_ssh_entry_boundaries() -> Result<()> {
    entry(false)
}

#[test]
#[ignore = "requires actual prepared public helper target; performs its unchanged Cargo build"]
fn actual_public_op_ssh_entry() -> Result<()> {
    entry(true)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    links: u64,
    size: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
fn identity(m: &Metadata) -> Identity {
    Identity {
        dev: m.dev(),
        ino: m.ino(),
        mode: m.mode(),
        uid: m.uid(),
        gid: m.gid(),
        links: m.nlink(),
        size: m.len(),
        mtime: (m.mtime(), m.mtime_nsec()),
        ctime: (m.ctime(), m.ctime_nsec()),
    }
}
struct Held {
    path: PathBuf,
    file: File,
    before: Identity,
    digest: String,
}
impl Held {
    fn open(path: PathBuf, max: u64) -> Result<Self> {
        require(
            path.is_absolute() && path.canonicalize()? == path,
            "held path is not canonical",
        )?;
        let file = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK,
            rustix::fs::Mode::empty(),
        )?);
        let m = file.metadata()?;
        require(
            m.is_file()
                && m.len() <= max
                && (m.uid() == rustix::process::geteuid().as_raw() || m.uid() == 0),
            "held input is not bounded owned regular data",
        )?;
        let before = identity(&m);
        let digest = Self::hash(&file, m.len())?;
        let result = Self {
            path,
            file,
            before,
            digest,
        };
        result.verify()?;
        Ok(result)
    }
    fn hash(file: &File, size: u64) -> Result<String> {
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut buffer = [0; 65536];
        while offset < size {
            let cap = (size - offset).min(buffer.len() as u64) as usize;
            let count = file.read_at(&mut buffer[..cap], offset)?;
            require(count > 0, "held input short read")?;
            hash.update(&buffer[..count]);
            offset += count as u64;
        }
        require(file.read_at(&mut [0; 1], size)? == 0, "held input grew")?;
        Ok(format!("{:x}", hash.finalize()))
    }
    fn verify(&self) -> Result<()> {
        require(
            identity(&self.file.metadata()?) == self.before
                && identity(&fs::symlink_metadata(&self.path)?) == self.before,
            "held identity changed",
        )?;
        require(
            Self::hash(&self.file, self.before.size)? == self.digest,
            "held bytes changed",
        )?;
        require(
            identity(&self.file.metadata()?) == self.before
                && identity(&fs::symlink_metadata(&self.path)?) == self.before,
            "held identity changed",
        )
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        self.verify()?;
        let mut bytes = vec![0; usize::try_from(self.before.size)?];
        let mut at = 0;
        while at < bytes.len() {
            let n = self.file.read_at(&mut bytes[at..], at as u64)?;
            require(n > 0, "held input short read")?;
            at += n;
        }
        self.verify()?;
        Ok(bytes)
    }
    fn elf(&self) -> Result<()> {
        let mut header = [0; 20];
        require(
            self.file.read_at(&mut header, 0)? == 20
                && &header[..6] == b"\x7fELF\x02\x01"
                && u16::from_le_bytes([header[18], header[19]]) == 62
                && self.before.mode & 0o111 != 0,
            "expected actual x86_64 ELF",
        )
    }
}
struct Directory {
    path: PathBuf,
    file: File,
    before: (u64, u64, u32, u32, u32),
}
impl Directory {
    fn id(m: &Metadata) -> (u64, u64, u32, u32, u32) {
        (m.dev(), m.ino(), m.mode(), m.uid(), m.gid())
    }
    fn open(path: PathBuf) -> Result<Self> {
        require(
            path.is_absolute() && path.canonicalize()? == path,
            "private directory is not canonical",
        )?;
        let file = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::DIRECTORY,
            rustix::fs::Mode::empty(),
        )?);
        let m = file.metadata()?;
        require(
            m.is_dir()
                && m.uid() == rustix::process::geteuid().as_raw()
                && m.mode() & 0o777 == 0o700,
            "expected owned private0700 directory",
        )?;
        Ok(Self {
            path,
            file,
            before: Self::id(&m),
        })
    }
    fn verify(&self) -> Result<()> {
        require(
            Self::id(&self.file.metadata()?) == self.before
                && Self::id(&fs::symlink_metadata(&self.path)?) == self.before
                && self.path.canonicalize()? == self.path,
            "private directory identity changed",
        )
    }
}

#[derive(Debug)]
struct Record {
    pid: u32,
    ppid: u32,
    pgid: u32,
    session: u32,
    start: u64,
    exe: Vec<u8>,
    cwd: Vec<u8>,
    argv: Vec<Vec<u8>>,
}
struct Decoder<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Decoder<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or("truncated recorder")?;
        let data = self.bytes.get(self.at..end).ok_or("truncated recorder")?;
        self.at = end;
        Ok(data)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn blob(&mut self) -> Result<Vec<u8>> {
        let n = self.u32()? as usize;
        require(n <= 8192, "recorder field exceeds bound")?;
        Ok(self.take(n)?.to_vec())
    }
}
impl Record {
    fn decode(bytes: &[u8], nonce: &str) -> Result<Self> {
        require(bytes.len() <= MARKER_CAP as usize, "recorder exceeds bound")?;
        let mut d = Decoder { bytes, at: 0 };
        require(d.take(MAGIC.len())? == MAGIC, "wrong recorder format")?;
        let pid = d.u32()?;
        let ppid = d.u32()?;
        let pgid = d.u32()?;
        let session = d.u32()?;
        let start = u64::from_le_bytes(d.take(8)?.try_into()?);
        require(d.blob()? == nonce.as_bytes(), "recorder nonce differs")?;
        let exe = d.blob()?;
        let cwd = d.blob()?;
        let n = d.u32()?;
        require((1..=32).contains(&n), "recorder argc exceeds bound")?;
        let argv = (0..n).map(|_| d.blob()).collect::<Result<Vec<_>>>()?;
        require(d.at == bytes.len(), "recorder has trailing data")?;
        require(
            pid > 1 && ppid > 0 && pgid > 1 && session > 0 && start > 0,
            "recorder process identity missing",
        )?;
        Ok(Self {
            pid,
            ppid,
            pgid,
            session,
            start,
            exe,
            cwd,
            argv,
        })
    }
    fn value(&self) -> Value {
        let hex = |bytes: &[u8]| bytes.iter().map(|v| format!("{v:02x}")).collect::<String>();
        json!({"pid":self.pid,"ppid":self.ppid,"pgid":self.pgid,"session":self.session,"start_ticks":self.start,"exe_hex":hex(&self.exe),"cwd_hex":hex(&self.cwd),"argv_hex":self.argv.iter().map(|v|hex(v)).collect::<Vec<_>>()})
    }
}
fn marker(root: &Path, expected: bool, nonce: &str) -> Result<Option<Record>> {
    let names = fs::read_dir(root)?
        .map(|row| row.map(|e| e.file_name()))
        .collect::<io::Result<BTreeSet<_>>>()?;
    let wanted = if expected {
        BTreeSet::from([OsString::from("exec.bin")])
    } else {
        BTreeSet::new()
    };
    require(names == wanted, "recorder marker roster differs")?;
    if !expected {
        return Ok(None);
    }
    let held = Held::open(root.join("exec.bin"), MARKER_CAP)?;
    require(
        held.before.uid == rustix::process::geteuid().as_raw()
            && held.before.links == 1
            && held.before.mode & 0o077 == 0,
        "recorder marker must be private single-link",
    )?;
    let decoded = Record::decode(&held.bytes()?, nonce)?;
    held.verify()?;
    Ok(Some(decoded))
}

fn trusted_host() -> Result<()> {
    for (key, value) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", "X64"),
    ] {
        require(
            std::env::var(key).as_deref() == Ok(value),
            "requires actual trusted hosted Linux X64",
        )?;
    }
    require(
        matches!(fs::symlink_metadata("/TICI"),Err(e) if e.kind()==io::ErrorKind::NotFound),
        "host entry must not select device auth",
    )?;
    let source = std::env::var("GITHUB_SHA")?;
    require(
        source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit()),
        "source SHA missing",
    )
}
struct Context {
    root: PathBuf,
    job: PathBuf,
    output: PathBuf,
    bin: PathBuf,
    cli: PathBuf,
    held: Vec<Held>,
    dirs: Vec<Directory>,
    recorder_sha: String,
}
impl Context {
    fn new(public: bool) -> Result<Self> {
        trusted_host()?;
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()?;
        require(
            env_path("CI296_SOURCE")? == root,
            "source root differs from compiled root",
        )?;
        let job = env_path("CI_JOB_TEMP")?;
        let proof = env_path("DEVELOPER_SSH_ENTRY_PROOF")?;
        require(
            proof == job.join("proof/developer-ssh-entry") && !proof.starts_with(&root),
            "entry proof is not the owned job path",
        )?;
        let mut dirs = vec![
            Directory::open(job.clone())?,
            Directory::open(proof.clone())?,
        ];
        let output = proof.join(if public { "public" } else { "native" });
        mkdir(&output)?;
        dirs.push(Directory::open(output.clone())?);
        let mut held = Vec::new();
        let owner = Held::open(job.join("owner"), 256)?;
        let expected = format!(
            "{}:{}:{}",
            std::env::var("GITHUB_RUN_ID")?,
            std::env::var("GITHUB_RUN_ATTEMPT")?,
            std::env::var("GITHUB_JOB")?
        );
        require(
            owner.bytes()? == format!("{expected}\n").as_bytes()
                || owner.bytes()? == expected.as_bytes(),
            "job owner differs",
        )?;
        held.push(owner);
        let sources: [(&str, &[u8]); 9] = [
            (
                "rust/crates/dev-tools/tests/developer_ssh_entry.rs",
                include_bytes!("developer_ssh_entry.rs"),
            ),
            (
                "rust/crates/dev-tools/tests/fixtures/developer-ssh-entry/exec_recorder.rs",
                include_bytes!("fixtures/developer-ssh-entry/exec_recorder.rs"),
            ),
            (
                "rust/crates/dev-tools/src/bin/developer_ssh.rs",
                include_bytes!("../src/bin/developer_ssh.rs"),
            ),
            (
                "rust/crates/dev-tools/src/developer_ssh.rs",
                include_bytes!("../src/developer_ssh.rs"),
            ),
            (
                "rust/crates/dev-tools/src/developer_ssh/cli.rs",
                include_bytes!("../src/developer_ssh/cli.rs"),
            ),
            (
                "rust/crates/dev-tools/src/developer_ssh/auth.rs",
                include_bytes!("../src/developer_ssh/auth.rs"),
            ),
            (
                "rust/crates/dev-tools/src/developer_ssh/http.rs",
                include_bytes!("../src/developer_ssh/http.rs"),
            ),
            ("tools/op.sh", include_bytes!("../../../../tools/op.sh")),
            (
                "rust/tools/run-developer-tool",
                include_bytes!("../../../tools/run-developer-tool"),
            ),
        ];
        for (name, bytes) in sources {
            let file = Held::open(root.join(name), 1024 * 1024)?;
            require(
                file.digest == sha(bytes),
                "current source differs from compiled input",
            )?;
            held.push(file);
        }
        let cli = PathBuf::from(env!("CARGO_BIN_EXE_developer-ssh"));
        require(
            cli.starts_with(job.join("build")),
            "compile-bound CLI must be in private build",
        )?;
        for path in [cli.clone(), std::env::current_exe()?] {
            let file = Held::open(path, FILE_CAP)?;
            file.elf()?;
            held.push(file);
        }
        let recorder_path = env_path("DEVELOPER_SSH_ENTRY_RECORDER")?;
        require(
            recorder_path.starts_with(job.join("build")),
            "recorder must be fresh private build output",
        )?;
        let recorder = Held::open(recorder_path, 16 * 1024 * 1024)?;
        recorder.elf()?;
        let recorder_sha = std::env::var("DEVELOPER_SSH_ENTRY_RECORDER_SHA256")?;
        require(
            recorder_sha.len() == 64
                && recorder_sha
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && recorder.digest == recorder_sha,
            "recorder ELF SHA differs",
        )?;
        let bin = output.join("bin");
        mkdir(&bin)?;
        dirs.push(Directory::open(bin.clone())?);
        create(&bin.join("ssh"), &recorder.bytes()?, 0o500)?;
        held.push(recorder);
        let copy = Held::open(bin.join("ssh"), 16 * 1024 * 1024)?;
        require(
            copy.digest == recorder_sha
                && copy.before.links == 1
                && copy.before.uid == rustix::process::geteuid().as_raw(),
            "private ssh is not exact single-link recorder",
        )?;
        held.push(copy);
        Ok(Self {
            root,
            job,
            output,
            bin,
            cli,
            held,
            dirs,
            recorder_sha,
        })
    }
    fn verify(&self) -> Result<()> {
        for directory in &self.dirs {
            directory.verify()?;
        }
        for file in &self.held {
            file.verify()?;
        }
        let names = fs::read_dir(&self.bin)?
            .map(|v| v.map(|v| v.file_name()))
            .collect::<io::Result<BTreeSet<_>>>()?;
        require(
            names == BTreeSet::from([OsString::from("ssh")]),
            "owned PATH roster changed",
        )
    }
    fn held_value(&self) -> Value {
        json!(self.held.iter().map(|v|json!({"path":v.path,"sha256":v.digest,"dev":v.before.dev,"ino":v.before.ino,"mode":v.before.mode,"uid":v.before.uid,"gid":v.before.gid,"nlink":v.before.links,"bytes":v.before.size,"mtime":v.before.mtime,"ctime":v.before.ctime})).collect::<Vec<_>>())
    }
}

fn owned_children() -> Result<Vec<rustix::process::Pid>> {
    let tasks = fs::read_dir("/proc/self/task")?
        .take(129)
        .collect::<io::Result<Vec<_>>>()?;
    require(tasks.len() <= 128, "worker task roster exceeds bound")?;
    let mut children = BTreeSet::new();
    for task in tasks {
        let names = match fs::read_to_string(task.path().join("children")) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        require(names.len() < 8192, "owned children roster exceeds bound")?;
        for name in names.split_whitespace() {
            children.insert(name.parse::<i32>()?);
            require(children.len() <= 256, "owned child count exceeded")?;
        }
    }
    children
        .into_iter()
        .map(|pid| {
            rustix::process::Pid::from_raw(pid).ok_or_else(|| "invalid owned child PID".into())
        })
        .collect()
}

/// 전용 integration target은 actual cohort를 직렬화하고 기존 자식이 없을 때만 소유한다.
/// worker가 timeout으로 사라져도 바깥 owner가 채택한 자식까지 ECHILD로 확인한다.
struct ReaperScope {
    previous: Option<rustix::process::Pid>,
    active: bool,
}
impl ReaperScope {
    fn enter() -> Result<Self> {
        require(
            owned_children()?.is_empty(),
            "preexisting child not owned by entry",
        )?;
        let previous = rustix::process::child_subreaper()?;
        rustix::process::set_child_subreaper(Some(rustix::process::getpid()))?;
        Ok(Self {
            previous,
            active: true,
        })
    }
    fn finish(&mut self) -> Result<usize> {
        let count = reap_owned()?;
        rustix::process::set_child_subreaper(self.previous)?;
        self.active = false;
        Ok(count)
    }
}
impl Drop for ReaperScope {
    fn drop(&mut self) {
        if self.active {
            let _ = self.finish();
        }
    }
}

/// 이 전용 owner의 직접/채택 자식만 회수한다. 다른 CI job의 프로세스는 조회하지 않는다.
fn reap_owned() -> Result<usize> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut count = 0;
    loop {
        match rustix::process::waitid(
            rustix::process::WaitId::All,
            rustix::process::WaitIdOptions::EXITED | rustix::process::WaitIdOptions::NOHANG,
        ) {
            Ok(Some(_)) => {
                count += 1;
                require(count <= 256, "owned child count exceeded")?;
            }
            Err(rustix::io::Errno::CHILD) => return Ok(count),
            Ok(None) => {
                require(Instant::now() < deadline, "owned descendants did not reap")?;
                // 이 owner만 wait하므로 아직 reap되지 않은 자식 PID는 재사용되지 않는다.
                for pid in owned_children()? {
                    match rustix::process::kill_process(pid, rustix::process::Signal::KILL) {
                        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                thread::sleep(Duration::from_millis(2));
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
struct Capture {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    pid: u32,
}
fn run_command(
    command: &mut Command,
    output: &Path,
    label: &str,
    timeout: Duration,
    limit: u64,
) -> Result<Capture> {
    let stdout = output.join(format!("{label}.stdout"));
    let stderr = output.join(format!("{label}.stderr"));
    let out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&stdout)?;
    let err = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&stderr)?;
    command.stdin(Stdio::null()).stdout(out).stderr(err);
    // 내부 command는 worker PGID를 그대로 상속한다. 바깥 NativeRunner가 소유한다.
    let mut owner = ChildOwner(command.spawn()?);
    let pid = owner.0.id();
    let deadline = Instant::now() + timeout;
    let result = (|| -> Result<ExitStatus> {
        loop {
            require(
                fs::metadata(&stdout)?.len() <= limit && fs::metadata(&stderr)?.len() <= limit,
                "command log exceeded bound",
            )?;
            if let Some(status) = owner.0.try_wait()? {
                return Ok(status);
            }
            require(Instant::now() < deadline, "entry command deadline exceeded")?;
            thread::sleep(Duration::from_millis(2));
        }
    })();
    drop(owner);
    let reaped = reap_owned();
    create(
        &output.join(format!("{label}.status.json")),
        &serde_json::to_vec(
            &json!({"pid":pid,"worker_pgid":rustix::process::getpgrp().as_raw_nonzero().get(),"code":result.as_ref().ok().and_then(ExitStatus::code),"error":result.as_ref().err().map(ToString::to_string),"reaped":reaped.is_ok(),"adopted_reaped":reaped.as_ref().ok()}),
        )?,
        0o600,
    )?;
    let status = result?;
    reaped?;
    let out = Held::open(stdout, limit)?;
    let err = Held::open(stderr, limit)?;
    let stdout = out.bytes()?;
    let stderr = err.bytes()?;
    out.verify()?;
    err.verify()?;
    Ok(Capture {
        code: status.code(),
        stdout,
        stderr,
        pid,
    })
}

#[derive(Debug)]
struct Request {
    raw: Vec<u8>,
    headers: BTreeMap<String, String>,
}
fn request(stream: &mut TcpStream) -> Result<Request> {
    let deadline = Instant::now() + Duration::from_secs(3);
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut raw = Vec::new();
    let mut byte = [0; 1];
    while !raw.ends_with(b"\r\n\r\n") {
        require(raw.len() < 16384, "HTTP headers exceed bound")?;
        stream.set_read_timeout(Some(
            deadline
                .checked_duration_since(Instant::now())
                .ok_or("HTTP header deadline exceeded")?,
        ))?;
        stream.read_exact(&mut byte)?;
        raw.push(byte[0]);
    }
    let text = std::str::from_utf8(&raw)?;
    let mut lines = text.split("\r\n");
    require(
        lines.next() == Some("GET /v1/me/devices HTTP/1.1"),
        "unexpected loopback request",
    )?;
    let mut headers = BTreeMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("malformed HTTP header")?;
        require(
            headers.len() < 64
                && headers
                    .insert(key.to_ascii_lowercase(), value.trim().to_string())
                    .is_none(),
            "duplicate/excess HTTP header",
        )?;
    }
    require(
        headers.get("content-length").is_none_or(|v| v == "0")
            && !headers.contains_key("transfer-encoding"),
        "unexpected request body",
    )?;
    Ok(Request { raw, headers })
}
struct Server {
    address: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<std::result::Result<Vec<Request>, String>>>,
}
impl Server {
    fn new(status: u16, body: &'static str) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?.to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            (|| -> Result<Vec<Request>> {
                let deadline = Instant::now() + Duration::from_secs(660);
                let mut rows = Vec::new();
                loop {
                    match listener.accept() {
                        Ok((mut stream, peer)) => {
                            require(
                                peer.ip().is_loopback() && rows.len() < 4,
                                "unexpected HTTP peer/count",
                            )?;
                            rows.push(request(&mut stream)?);
                            write!(stream,"HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())?;
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            // 자식 reap 뒤에도 이미 queue에 들어온 연결은 위에서 기록한다.
                            if flag.load(Ordering::Acquire) {
                                break;
                            }
                            require(
                                Instant::now() < deadline,
                                "loopback server deadline exceeded",
                            )?;
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Ok(rows)
            })()
            .map_err(|error| error.to_string())
        });
        Ok(Self {
            address,
            stop,
            thread: Some(thread),
        })
    }
    fn finish(&mut self) -> Result<Vec<Request>> {
        self.stop.store(true, Ordering::Release);
        self.thread
            .take()
            .ok_or("HTTP server already joined")?
            .join()
            .map_err(|_| "HTTP server panicked")?
            .map_err(Into::into)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Case {
    Alias,
    Duplicate,
    Debug,
    Unauthorized,
    Help,
    InvalidPort,
    Missing,
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Alias => "alias",
            Self::Duplicate => "duplicate",
            Self::Debug => "debug",
            Self::Unauthorized => "unauthorized",
            Self::Help => "help",
            Self::InvalidPort => "invalid-port",
            Self::Missing => "missing",
        }
    }
    fn parser(self) -> bool {
        matches!(self, Self::Help | Self::InvalidPort | Self::Missing)
    }
    fn exec(self) -> bool {
        matches!(self, Self::Alias | Self::Debug)
    }
}
const DEVICES: &str = r#"[{"dongle_id":"0000000000000001","alias":"Entry Car"}]"#;
const DUPLICATES: &str = r#"[{"dongle_id":"0000000000000001","alias":"First Car"},{"dongle_id":"0000000000000002","alias":"Second Car"}]"#;
fn path_text(path: &Path) -> Result<String> {
    Ok(path.to_str().ok_or("entry path must be UTF-8")?.into())
}
fn nonce() -> Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
fn public_environment(
    ctx: &mut Context,
    env: &mut BTreeMap<OsString, OsString>,
) -> Result<PathBuf> {
    let rustup = ctx.job.join("rustup-home");
    let sdk = rustup.join("toolchains/1.94.0-x86_64-unknown-linux-gnu");
    let rustup_bin = ctx.job.join("cargo-home/bin/rustup");
    for (key, path) in [
        ("CI_PREPARED_RUSTUP_ROOT", &rustup),
        ("CI_PREPARED_RUST_ROOT", &sdk),
        ("CI_PREPARED_RUSTUP_BIN", &rustup_bin),
    ] {
        require(
            env_path(key)? == *path && path.canonicalize()? == *path,
            "public SDK binding differs",
        )?;
    }
    for tool in ["cargo", "rustc", "rustdoc", "clippy-driver"] {
        let held = Held::open(sdk.join("bin").join(tool), FILE_CAP)?;
        held.elf()?;
        ctx.held.push(held);
    }
    let held = Held::open(rustup_bin.clone(), FILE_CAP)?;
    held.elf()?;
    ctx.held.push(held);
    for (key, path) in [
        ("CARGO_HOME", ctx.job.join("build/cargo-home")),
        ("CARGO_TARGET_DIR", ctx.job.join("build/target")),
    ] {
        require(
            env_path(key)? == path
                && path.canonicalize()? == path
                && fs::metadata(&path)?.uid() == rustix::process::geteuid().as_raw(),
            "public Cargo directory differs",
        )?;
        env.insert(key.into(), path.into_os_string());
    }
    let public_cli = ctx
        .job
        .join("build/target/x86_64-unknown-linux-gnu/debug/developer-ssh");
    let held = Held::open(public_cli.clone(), FILE_CAP)?;
    held.elf()?;
    ctx.held.push(held);
    env.insert(
        "PATH".into(),
        format!(
            "{}:{}:{}:/usr/bin:/bin",
            path_text(&ctx.bin)?,
            path_text(&sdk.join("bin"))?,
            path_text(rustup_bin.parent().ok_or("rustup parent missing")?)?
        )
        .into(),
    );
    for (key, value) in [
        ("SHELL", "/bin/bash".into()),
        ("RUSTUP_HOME", rustup.into_os_string()),
        ("RUSTUP_TOOLCHAIN", "1.94.0".into()),
        ("RUSTUP_AUTO_INSTALL", "0".into()),
        ("RUSTC", sdk.join("bin/rustc").into_os_string()),
        ("RUSTDOC", sdk.join("bin/rustdoc").into_os_string()),
        ("CARGO_NET_OFFLINE", "true".into()),
        ("GIT_OPTIONAL_LOCKS", "0".into()),
    ] {
        env.insert(key.into(), value);
    }
    Ok(rustup_bin)
}
fn run_case(ctx: &mut Context, case: Case, public: bool) -> Result<Value> {
    let directory = ctx.output.join(case.name());
    mkdir(&directory)?;
    let home = directory.join("home");
    mkdir(&home)?;
    let auth = home.join(format!(".comma{PREFIX}"));
    mkdir(&auth)?;
    let markers = directory.join("markers");
    mkdir(&markers)?;
    for path in [&directory, &home, &auth, &markers] {
        ctx.dirs.push(Directory::open(path.clone())?);
    }
    let nonce = nonce()?;
    let auth_path = auth.join("auth.json");
    if case.parser() {
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &auth_path,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )?;
    } else {
        create(
            &auth_path,
            &serde_json::to_vec(&json!({"access_token":format!("inert-entry-{nonce}")}))?,
            0o600,
        )?;
        ctx.held.push(Held::open(auth_path.clone(), 4096)?);
    }
    let auth_before = identity(&fs::symlink_metadata(&auth_path)?);
    let key = directory.join("inert key");
    create(&key, b"INERT NOT AN SSH KEY\n", 0o600)?;
    ctx.held.push(Held::open(key.clone(), 4096)?);
    let mut env = BTreeMap::<OsString, OsString>::from([
        ("HOME".into(), home.into_os_string()),
        ("OPENPILOT_PREFIX".into(), PREFIX.into()),
        ("API_HOST".into(), OsString::new()),
        ("PATH".into(), ctx.bin.clone().into_os_string()),
        ("NO_PROXY".into(), "127.0.0.1".into()),
        ("no_proxy".into(), "127.0.0.1".into()),
        ("LANG".into(), "C.UTF-8".into()),
        ("LC_ALL".into(), "C.UTF-8".into()),
        (
            "DEVELOPER_SSH_ENTRY_MARKERS".into(),
            markers.clone().into_os_string(),
        ),
        ("DEVELOPER_SSH_ENTRY_NONCE".into(), nonce.clone().into()),
    ]);
    let rustup_bin = if public {
        Some(public_environment(ctx, &mut env)?)
    } else {
        None
    };
    if let Some(rustup) = rustup_bin {
        let mut probe = Command::new(rustup);
        probe
            .args(["run", "1.94.0", "rustc", "-vV"])
            .env_clear()
            .envs(&env)
            .current_dir(&ctx.root);
        let observed = run_command(
            &mut probe,
            &directory,
            "sdk-probe",
            Duration::from_secs(30),
            65536,
        )?;
        require(
            observed.code == Some(0)
                && std::str::from_utf8(&observed.stdout)?
                    .lines()
                    .any(|v| v == "release: 1.94.0")
                && std::str::from_utf8(&observed.stdout)?
                    .lines()
                    .any(|v| v == "host: x86_64-unknown-linux-gnu"),
            "public SDK not already prepared; no install permitted",
        )?;
    }
    let (status, body) = match case {
        Case::Duplicate => (200, DUPLICATES),
        Case::Unauthorized => (401, r#"{"error":"unauthorized","description":"synthetic"}"#),
        _ => (200, DEVICES),
    };
    let mut server = Server::new(status, body)?;
    env.insert(
        "API_HOST".into(),
        format!("http://{}", server.address).into(),
    );
    let key = path_text(&key)?;
    let args = match case {
        Case::Help => vec!["--help".into()],
        Case::InvalidPort => vec!["--port".into(), "not-port".into(), "entry".into()],
        Case::Missing => vec![],
        Case::Debug => vec![
            "--debug".into(),
            "--host".into(),
            "relay.invalid".into(),
            "--port".into(),
            "+002222".into(),
            "--key".into(),
            key.clone(),
            "entry".into(),
        ],
        _ => vec![
            "--key".into(),
            key.clone(),
            if case == Case::Duplicate {
                "car".into()
            } else {
                "e n t r y c a r".into()
            },
        ],
    };
    let cwd = if public { &ctx.root } else { &directory };
    let mut command = if public {
        let bash = Path::new("/bin/bash").canonicalize()?;
        let held = Held::open(bash.clone(), 16 * 1024 * 1024)?;
        held.elf()?;
        ctx.held.push(held);
        let mut command = Command::new(bash);
        command
            .arg(ctx.root.join("tools/op.sh"))
            .args(["--no-verify", "ssh"]);
        command
    } else {
        Command::new(&ctx.cli)
    };
    command.args(&args).current_dir(cwd).env_clear().envs(&env);
    ctx.verify()?;
    let result = run_command(
        &mut command,
        &directory,
        "entry",
        Duration::from_secs(if public { 600 } else { 15 }),
        if public { 4 * 1024 * 1024 } else { 65536 },
    );
    let requests = server.finish()?;
    let raw:Vec<_>=requests.iter().map(|r|json!({"raw_hex":r.raw.iter().map(|b|format!("{b:02x}")).collect::<String>(),"headers":r.headers})).collect();
    create(
        &directory.join("http.json"),
        &serde_json::to_vec(&raw)?,
        0o600,
    )?;
    let capture = result?;
    require(
        identity(&fs::symlink_metadata(&auth_path)?) == auth_before,
        "token input changed",
    )?;
    require(
        requests.len() == usize::from(!case.parser()),
        "unexpected HTTP count",
    )?;
    if let Some(request) = requests.first() {
        require(
            request.headers.get("authorization") == Some(&format!("JWT inert-entry-{nonce}"))
                && request.headers.get("user-agent").map(String::as_str) == Some("OpenpilotTools")
                && request.headers.get("host") == Some(&server.address),
            "actual NativeTransport headers differ",
        )?;
    }
    let recorded = marker(&markers, case.exec(), &nonce)?;
    if let Some(record) = &recorded {
        require(
            record.exe == ctx.bin.join("ssh").as_os_str().as_bytes()
                && record.cwd == cwd.as_os_str().as_bytes()
                && record.pgid == u32::try_from(rustix::process::getpgrp().as_raw_pid())?,
            "recorder path/cwd/owned PGID differs",
        )?;
        if !public {
            require(
                record.pid == capture.pid,
                "Plan::exec did not retain actual CLI PID",
            )?;
        }
        require(
            matches!(
                rustix::process::waitpid(
                    Some(
                        rustix::process::Pid::from_raw(i32::try_from(record.pid)?)
                            .ok_or("bad marker PID")?
                    ),
                    rustix::process::WaitOptions::NOHANG
                ),
                Err(rustix::io::Errno::CHILD)
            ),
            "recorded child not reaped",
        )?;
        require(
            !Path::new(&format!("/proc/{}", record.pid)).exists(),
            "recorded process remains",
        )?;
        let (host, port) = if case == Case::Debug {
            ("relay.invalid", "2222")
        } else {
            ("ssh.comma.ai", "22")
        };
        let mut expected = vec![
            "ssh".to_string(),
            "-i".into(),
            key.clone(),
            "-o".into(),
            format!("ProxyCommand=ssh -i {key} -W %h:%p -p %p %h@{host}"),
            "-p".into(),
            port.into(),
        ];
        if case == Case::Debug {
            expected.push("-v".into());
        }
        expected.push("comma@comma-0000000000000001".into());
        require(
            record.argv
                == expected
                    .iter()
                    .map(|v| v.as_bytes().to_vec())
                    .collect::<Vec<_>>(),
            "actual ssh argv bytes differ",
        )?;
        let connecting =
            format!("connecting to Entry Car (0000000000000001) through {host}:{port} ...\n");
        if public {
            require(
                std::str::from_utf8(&capture.stdout)?
                    .lines()
                    .filter(|line| *line == connecting.trim_end())
                    .count()
                    == 1,
                "public helper did not reach native connecting output",
            )?;
        } else {
            let expected = if case == Case::Debug {
                format!("{connecting}ssh -i '{key}' -o 'ProxyCommand=ssh -i {key} -W %h:%p -p %p %h@relay.invalid' -p 2222 -v comma@comma-0000000000000001\n")
            } else {
                connecting
            };
            require(
                capture.stdout == expected.as_bytes() && capture.stderr.is_empty(),
                "native connecting/debug bytes differ",
            )?;
        }
        require(capture.code == Some(0), "recorder entry did not exit0")?;
    } else {
        match case {
        Case::Duplicate=>require(capture.code==Some(1) && capture.stdout.is_empty() && capture.stderr==b"failed to look up dongle id for \"car\"\nfound multiple matches:\n  \"First Car\" (0000000000000001)\n  \"Second Car\" (0000000000000002)\n","duplicate rejection differs")?,
        Case::Unauthorized=>require(capture.code==Some(1) && capture.stdout.is_empty() && capture.stderr==b"Unauthorized. Authenticate with tools/lib/auth.py\n","HTTP401 rejection differs")?,
        Case::Help=>require(capture.code==Some(0) && capture.stderr.is_empty() && capture.stdout.starts_with(format!("usage: {} [-h]",path_text(&ctx.cli)?).as_bytes()),"help effects differ")?,
        Case::InvalidPort=>require(capture.code==Some(2) && capture.stdout.is_empty() && std::str::from_utf8(&capture.stderr)?.contains("invalid int value: \"not-port\""),"invalid port parser effects differ")?,
        Case::Missing=>require(capture.code==Some(2) && capture.stdout.is_empty() && std::str::from_utf8(&capture.stderr)?.contains("unrecognized arguments"),"missing positional effects differ")?,
        _=>return Err("required recorder missing".into()),
    }
    }
    ctx.verify()?;
    let value = json!({"case":case.name(),"nonce":nonce,"exit":capture.code,"http":requests.len(),"recorder":recorded.as_ref().map(Record::value),"token_fifo":case.parser(),"token_input_unchanged":true,"reaped":true});
    create(
        &directory.join("case.json"),
        &serde_json::to_vec(&value)?,
        0o600,
    )?;
    Ok(value)
}

fn worker(public: bool) -> Result<()> {
    require(
        rustix::process::getpgrp() == rustix::process::getpid(),
        "worker must own its NativeRunner PGID",
    )?;
    rustix::process::set_child_subreaper(Some(rustix::process::getpid()))?;
    let mut ctx = Context::new(public)?;
    let primary_count = ctx.held.len();
    create(
        &ctx.output.join("held.before.json"),
        &serde_json::to_vec(&ctx.held_value())?,
        0o600,
    )?;
    let mut rows = Vec::new();
    let result = (|| -> Result<()> {
        let cases: &[_] = if public {
            &[Case::Alias]
        } else {
            &[
                Case::Alias,
                Case::Duplicate,
                Case::Debug,
                Case::Unauthorized,
                Case::Help,
                Case::InvalidPort,
                Case::Missing,
            ]
        };
        for case in cases {
            rows.push(run_case(&mut ctx, *case, public)?);
        }
        Ok(())
    })();
    let reaped = reap_owned();
    let sealed = ctx.verify();
    let all_held = ctx.held_value();
    let after = if sealed.is_ok() {
        json!(all_held
            .as_array()
            .ok_or("held roster missing")?
            .iter()
            .take(primary_count)
            .cloned()
            .collect::<Vec<_>>())
    } else {
        json!({"verified":false,"error":sealed.as_ref().err().map(ToString::to_string)})
    };
    create(
        &ctx.output.join("held.after.json"),
        &serde_json::to_vec(&after)?,
        0o600,
    )?;
    create(
        &ctx.output.join("per-use-held.json"),
        &serde_json::to_vec(
            &json!({"before_each_use":all_held,"verified_after":sealed.is_ok(),"error":sealed.as_ref().err().map(ToString::to_string)}),
        )?,
        0o600,
    )?;
    let complete = result.is_ok() && reaped.is_ok() && sealed.is_ok();
    create(
        &ctx.output.join("summary.json"),
        &serde_json::to_vec(
            &json!({"schema_version":1,"complete":complete,"mode":if public {"public"} else {"native"},"source_sha":std::env::var("GITHUB_SHA")?,"source_binding":"parent-source-seal plus compile-bound held9","recorder_sha256":ctx.recorder_sha,"cases":rows,"error":result.as_ref().err().map(ToString::to_string),"held_verified":sealed.is_ok(),"reaped":reaped.is_ok(),"real_ssh":false,"installed_provider_closure":false,"retirement_credit":0}),
        )?,
        0o600,
    )?;
    result?;
    reaped?;
    sealed?;
    println!(
        "\nDEVELOPER_SSH_ENTRY_COMPLETE mode={} calls={} http={} exec={} reaped=true",
        if public { "public" } else { "native" },
        if public { 1 } else { 7 },
        if public { 1 } else { 4 },
        if public { 1 } else { 2 }
    );
    Ok(())
}
fn entry(public: bool) -> Result<()> {
    trusted_host()?;
    let _serial = COHORT_LOCK
        .lock()
        .map_err(|_| "SSH entry cohort lock poisoned")?;
    let mode = if public { "public" } else { "native" };
    if let Some(value) = std::env::var_os("DEVELOPER_SSH_ENTRY_WORKER") {
        require(value == mode, "unexpected worker mode")?;
        return worker(public);
    }
    let current = Held::open(std::env::current_exe()?, FILE_CAP)?;
    current.elf()?;
    let env_tool = Held::open(Path::new("/usr/bin/env").canonicalize()?, 16 * 1024 * 1024)?;
    env_tool.elf()?;
    let mut argv = vec![env_tool.path.clone().into_os_string(), "-i".into()];
    for key in [
        "GITHUB_ACTIONS",
        "GITHUB_REPOSITORY",
        "RUNNER_ENVIRONMENT",
        "RUNNER_OS",
        "RUNNER_ARCH",
        "GITHUB_SHA",
        "GITHUB_RUN_ID",
        "GITHUB_RUN_ATTEMPT",
        "GITHUB_JOB",
        "CI296_SOURCE",
        "CI_JOB_TEMP",
        "DEVELOPER_SSH_ENTRY_PROOF",
        "DEVELOPER_SSH_ENTRY_RECORDER",
        "DEVELOPER_SSH_ENTRY_RECORDER_SHA256",
    ] {
        let value = std::env::var_os(key).ok_or_else(|| format!("missing {key}"))?;
        let mut assignment = OsString::from(format!("{key}="));
        assignment.push(value);
        argv.push(assignment);
    }
    if public {
        for key in [
            "CI_PREPARED_RUST_ROOT",
            "CI_PREPARED_RUSTUP_ROOT",
            "CI_PREPARED_RUSTUP_BIN",
            "CARGO_HOME",
            "CARGO_TARGET_DIR",
        ] {
            let mut value = OsString::from(format!("{key}="));
            value.push(std::env::var_os(key).ok_or_else(|| format!("missing {key}"))?);
            argv.push(value);
        }
    }
    argv.push(format!("DEVELOPER_SSH_ENTRY_WORKER={mode}").into());
    argv.extend([
        current.path.clone().into_os_string(),
        "--ignored".into(),
        "--exact".into(),
        if public {
            "actual_public_op_ssh_entry".into()
        } else {
            "actual_native_ssh_entry_boundaries".into()
        },
        "--nocapture".into(),
        "--test-threads=1".into(),
    ]);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(if public { 720 } else { 180 }));
    spec.stdout_limit = 128 * 1024;
    spec.stderr_limit = 128 * 1024;
    let proof = env_path("DEVELOPER_SSH_ENTRY_PROOF")?;
    require(
        proof == env_path("CI_JOB_TEMP")?.join("proof/developer-ssh-entry"),
        "supervisor proof binding differs",
    )?;
    let proof = Directory::open(proof)?;
    let mut reaper = ReaperScope::enter()?;
    let result = NativeRunner.run(&spec).map_err(|error| error.to_string());
    let reaped = reaper.finish();
    proof.verify()?;
    let output = Directory::open(proof.path.join(mode))?;
    create(
        &output.path.join("supervisor.json"),
        &serde_json::to_vec(&json!({
            "mode":mode,
            "worker_code":result.as_ref().ok().map(|value|value.code),
            "worker_error":result.as_ref().err(),
            "all_owned_children_reaped":reaped.is_ok(),
            "adopted_reaped":reaped.as_ref().ok(),
            "reap_error":reaped.as_ref().err().map(ToString::to_string),
        }))?,
        0o600,
    )?;
    proof.verify()?;
    output.verify()?;
    current.verify()?;
    env_tool.verify()?;
    reaped?;
    let captured = result.map_err(|error| -> Box<dyn Error> { error.into() })?;
    print!("{}", std::str::from_utf8(&captured.stdout)?);
    eprint!("{}", std::str::from_utf8(&captured.stderr)?);
    require(
        captured.code == 0,
        "isolated actual SSH entry worker failed",
    )?;
    let expected = format!(
        "DEVELOPER_SSH_ENTRY_COMPLETE mode={mode} calls={} http={} exec={} reaped=true",
        if public { 1 } else { 7 },
        if public { 1 } else { 4 },
        if public { 1 } else { 2 }
    );
    require(
        std::str::from_utf8(&captured.stdout)?
            .lines()
            .filter(|line| *line == expected)
            .count()
            == 1,
        "actual entry completion marker missing/duplicated",
    )
}
