//! #399: private filesystem and subprocess controls; no USB, model or device I/O.
#![cfg(target_os = "linux")]
use openpilot_jetlink::{
    contract::Identity,
    owner::{Backend, Server},
    Deadline, Error,
};
use std::{
    fs,
    io::Write,
    os::{
        fd::OwnedFd,
        unix::{
            fs::MetadataExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
struct Fixture {
    fail: bool,
    closed: Arc<AtomicBool>,
}
impl Backend for Fixture {
    fn connect(&mut self) -> Result<Identity, Error> {
        if self.fail {
            Err(Error::Contract("owned connect failure"))
        } else {
            Ok(Identity::new())
        }
    }
    fn infer(
        &mut self,
        _: u32,
        _: &[u8],
        _: &[f32],
        _: Deadline,
        _: bool,
    ) -> Result<Vec<f32>, Error> {
        Err(Error::Closed)
    }
    fn dead(&self) -> bool {
        self.fail
    }
    fn close(&mut self) {
        self.closed.store(true, Ordering::Release);
    }
}
fn fixture(fail: bool) -> Fixture {
    Fixture {
        fail,
        closed: Arc::new(AtomicBool::new(false)),
    }
}
fn wait_for(server: &Server, failed: bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if if failed {
            !server.error.lock().unwrap().is_empty()
        } else {
            server.ready.load(Ordering::Acquire)
        } {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "owned worker did not acknowledge startup"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn failed_startup_preserves_foreign_listener(fail_connect: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("foreign.sock");
    let foreign = UnixListener::bind(&path).unwrap();
    let identity = fs::symlink_metadata(&path).unwrap();
    let backend = fixture(fail_connect);
    let closed = Arc::clone(&backend.closed);
    let mut server = Server::spawn(backend, &path).unwrap();
    wait_for(&server, true);
    server.stop(Duration::from_secs(1)).unwrap();
    assert!(closed.load(Ordering::Acquire));
    let current =
        fs::symlink_metadata(&path).expect("foreign listener was removed by failed startup");
    assert_eq!(
        (current.dev(), current.ino()),
        (identity.dev(), identity.ino())
    );
    let _client = UnixStream::connect(&path).unwrap();
    let _accepted = foreign.accept().unwrap();
}
#[test]
fn failed_connect_preserves_foreign_listener() {
    failed_startup_preserves_foreign_listener(true);
}
#[test]
fn failed_bind_preserves_foreign_listener() {
    failed_startup_preserves_foreign_listener(false);
}
#[test]
fn ready_owner_cleanup_preserves_a_replacement_listener() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owner.sock");
    let mut server = Server::spawn(fixture(false), &path).unwrap();
    wait_for(&server, false);
    fs::remove_file(&path).unwrap();
    let foreign = UnixListener::bind(&path).unwrap();
    let identity = fs::symlink_metadata(&path).unwrap();
    server.stop(Duration::from_secs(1)).unwrap();
    drop(server);
    let current = fs::symlink_metadata(&path).expect("replacement listener was removed");
    assert_eq!(
        (current.dev(), current.ino()),
        (identity.dev(), identity.ino())
    );
    let _client = UnixStream::connect(&path).unwrap();
    let _accepted = foreign.accept().unwrap();
}
fn write(path: &Path, value: &serde_json::Value) {
    let temporary = path.with_extension("json.part");
    let mut file = fs::File::create(&temporary).unwrap();
    serde_json::to_writer(&mut file, value).unwrap();
    file.flush().unwrap();
    drop(file);
    fs::rename(temporary, path).unwrap();
}
// Not skipped coverage: the parent regressions invoke this subprocess entry
// point explicitly for their individual modes.
#[test]
#[ignore = "owned subprocess entry point, invoked by its parent regression"]
fn owner_lifecycle_child() {
    let root =
        PathBuf::from(std::env::var_os("JETLINK_OWNER_CHILD_ROOT").expect("owned child root"));
    let mode = std::env::var("JETLINK_OWNER_CHILD_MODE").unwrap();
    let backend = fixture(mode != "healthy");
    let closed = Arc::clone(&backend.closed);
    let mut server = Server::spawn(backend, &root.join("owner.sock")).unwrap();
    wait_for(&server, mode != "healthy");
    let error = Arc::clone(&server.error);
    write(
        &root.join("entered.json"),
        &serde_json::json!({"pid":std::process::id(),"mode":mode}),
    );
    let start = Instant::now();
    if mode == "drop" {
        drop(server);
    } else {
        server.stop(Duration::from_millis(100)).unwrap();
        drop(server);
    }
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "stop/drop exceeded bounded host allowance"
    );
    assert!(closed.load(Ordering::Acquire));
    assert!(!root.join("owner.sock").exists());
    write(
        &root.join("returned.json"),
        &serde_json::json!({"mode":mode,"error":error.lock().unwrap().as_str()}),
    );
}
struct ChildOwner {
    child: Option<Child>,
    identity: OwnedFd,
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ =
                rustix::process::pidfd_send_signal(&self.identity, rustix::process::Signal::KILL);
            let _ = child.wait();
        }
    }
}
fn full_stderr(mode: &str) {
    use rustix::{
        fs::{fcntl_getfl, fcntl_setfl, OFlags},
        process::{pidfd_open, waitid, Pid, PidfdFlags, WaitId, WaitIdOptions},
    };
    let base = std::env::var_os("JETLINK_OWNER_TEST_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&base).unwrap();
    let root = tempfile::Builder::new()
        .prefix("jetlink-399-")
        .tempdir_in(base)
        .unwrap()
        .keep();
    let (_reader, mut writer) = std::io::pipe().unwrap();
    let flags = fcntl_getfl(&writer).unwrap();
    fcntl_setfl(&writer, flags | OFlags::NONBLOCK).unwrap();
    loop {
        match writer.write(&[b'x'; 4096]) {
            Ok(n) => assert!(n > 0),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => (),
            Err(e) => panic!("{e}"),
        }
    }
    fcntl_setfl(&writer, flags).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "owner_lifecycle_child",
            "--ignored",
            "--nocapture",
        ])
        .env("JETLINK_OWNER_CHILD_ROOT", &root)
        .env("JETLINK_OWNER_CHILD_MODE", mode)
        .stdout(Stdio::null())
        .stderr(Stdio::from(writer))
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut owner = ChildOwner {
        child: Some(child),
        identity,
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("entered.json").is_file() {
        assert!(
            Instant::now() < deadline,
            "{mode}: worker startup acknowledgment absent; {}",
            root.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
    let entered: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("entered.json")).unwrap()).unwrap();
    assert_eq!(entered["pid"], owner.child.as_ref().unwrap().id());
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if waitid(
            WaitId::Pid(Pid::from_child(owner.child.as_ref().unwrap())),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .is_some()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{mode}: blocked stderr extended stop/drop; evidence {}",
            root.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        owner.child.take().unwrap().wait().unwrap().success(),
        "{mode}: owned child failed; {}",
        root.display()
    );
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("returned.json")).unwrap()).unwrap();
    assert_eq!(result["mode"], mode);
    assert_eq!(
        result["error"],
        if mode == "healthy" {
            ""
        } else {
            "Jetlink contract: owned connect failure"
        }
    );
    fs::remove_dir_all(&root).unwrap();
    // The pipe remains full for the entire child lifetime. No draining is
    // used to manufacture a bounded shutdown or hide a blocking diagnostic.
}
#[test]
fn full_stderr_cannot_extend_failed_stop() {
    full_stderr("failure");
}
#[test]
fn full_stderr_cannot_extend_drop() {
    full_stderr("drop");
}
#[test]
fn full_stderr_healthy_owner_stops() {
    full_stderr("healthy");
}
