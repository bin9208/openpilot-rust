//! Invocation-bound ownership of an IPC pathname reported during live readiness.
//! A private channel transfers identity before the worker can start a candidate.
//! No mutable output receipt or post-teardown observation establishes ownership.
use crate::seal::Result;
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const LIMIT: usize = 1024;
const STEP: Duration = Duration::from_millis(20);
const GATE: Duration = Duration::from_secs(10);

fn read_line(stream: &mut UnixStream, stop: &AtomicBool) -> Result<Vec<u8>> {
    stream.set_read_timeout(Some(STEP))?;
    let end = Instant::now() + GATE;
    let mut bytes = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("endpoint identity acknowledgement interrupted".into());
        }
        if Instant::now() >= end {
            return Err("endpoint identity acknowledgement timed out".into());
        }
        let mut part = [0; 128];
        match stream.read(&mut part) {
            Ok(0) => return Err("endpoint identity acknowledgement closed".into()),
            Ok(n) => {
                bytes.extend_from_slice(&part[..n]);
                if bytes.len() > LIMIT {
                    return Err("endpoint identity message too large".into());
                }
                if let Some(index) = bytes.iter().position(|&byte| byte == b'\n') {
                    if index + 1 != bytes.len() {
                        return Err("endpoint identity message has trailing bytes".into());
                    }
                    return Ok(bytes);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
}
struct Held {
    // O_PATH keeps the filesystem inode allocated even after unlink or child
    // death. Socket descriptors themselves have a different sockfs identity.
    file: File,
    device: u64,
    inode: u64,
}
impl Held {
    fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.file_type().is_socket() {
            return Err("endpoint identity is not a socket".into());
        }
        Ok(Self {
            file,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

pub(crate) struct EndpointOwner {
    endpoint: PathBuf,
    _home: tempfile::TempDir,
    channel: PathBuf,
    stop: Arc<AtomicBool>,
    identity: Arc<Mutex<Option<Held>>>,
    receiver: Option<JoinHandle<std::result::Result<(), String>>>,
}
impl EndpointOwner {
    pub(crate) fn new(endpoint: &Path, nonce: &str) -> Result<Self> {
        match fs::symlink_metadata(endpoint) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err("endpoint already exists before owned worker launch".into()),
        }
        let home = tempfile::Builder::new()
            .prefix("qa-endpoint-")
            .tempdir_in("/tmp")?;
        let channel = home.path().join("identity");
        let listener = UnixListener::bind(&channel)?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let identity = Arc::new(Mutex::new(None));
        let receiver_stop = stop.clone();
        let receiver_identity = identity.clone();
        let receiver_endpoint = endpoint.to_owned();
        let expected = nonce.to_owned();
        let receiver = thread::spawn(move || {
            let run = || -> Result<()> {
                let mut stream = loop {
                    if receiver_stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(STEP)
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                let message: Value =
                    serde_json::from_slice(&read_line(&mut stream, &receiver_stop)?)?;
                if message["nonce"].as_str() != Some(expected.as_str()) {
                    return Err("endpoint identity nonce mismatch".into());
                }
                let held = Held::open(&receiver_endpoint)?;
                if message["device"].as_u64() != Some(held.device)
                    || message["inode"].as_u64() != Some(held.inode)
                {
                    return Err("endpoint identity changed before acknowledgement".into());
                }
                // Publish ownership before acknowledgement: a candidate that
                // follows the acknowledgement cannot replace an unrecorded inode.
                *receiver_identity
                    .lock()
                    .map_err(|_| "endpoint identity lock poisoned")? = Some(held);
                stream.set_write_timeout(Some(STEP))?;
                stream.write_all(b"1\n")?;
                Ok(())
            };
            run().map_err(|error| error.to_string())
        });
        Ok(Self {
            endpoint: endpoint.into(),
            _home: home,
            channel,
            stop,
            identity,
            receiver: Some(receiver),
        })
    }
    pub(crate) fn channel(&self) -> &Path {
        &self.channel
    }
    pub(crate) fn verify(&self) -> Result<()> {
        if self
            .identity
            .lock()
            .map_err(|_| "endpoint identity lock poisoned")?
            .is_none()
        {
            return Err("worker did not establish live endpoint ownership".into());
        }
        Ok(())
    }
}
impl Drop for EndpointOwner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(receiver) = self.receiver.take() {
            let _ = receiver.join();
        }
        let held = self
            .identity
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(held) = held {
            if let Ok(metadata) = fs::symlink_metadata(&self.endpoint) {
                if metadata.file_type().is_socket()
                    && (metadata.dev(), metadata.ino()) == (held.device, held.inode)
                {
                    let _ = fs::remove_file(&self.endpoint);
                }
            }
            // Preserve the inode reservation through the final comparison/unlink.
            drop(held.file);
        }
        // Before an authenticated readiness message no pathname is ours to
        // unlink. Abrupt death in that bind-to-message window can leave an owned
        // socket; never trade that bounded limitation for deleting a foreign one.
    }
}

/// Called by the trusted worker after collector readiness and before candidate
/// launch. The worker cannot proceed until its outer owner holds the inode.
pub(crate) fn acknowledge(
    endpoint: &Path,
    channel: &Path,
    nonce: &str,
    cancelled: &AtomicBool,
) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("endpoint identity interrupted".into());
    }
    let metadata = fs::symlink_metadata(endpoint)?;
    if !metadata.file_type().is_socket() {
        return Err("ready endpoint is not a socket".into());
    }
    let mut stream = UnixStream::connect(channel)?;
    stream.set_write_timeout(Some(GATE))?;
    let mut message =
        serde_json::to_vec(&json!({"nonce":nonce,"device":metadata.dev(),"inode":metadata.ino()}))?;
    message.push(b'\n');
    if message.len() > LIMIT {
        return Err("endpoint identity message too large".into());
    }
    stream.write_all(&message)?;
    if read_line(&mut stream, cancelled)? != b"1\n" {
        return Err("endpoint identity invalid acknowledgement".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{acknowledge, EndpointOwner};
    use std::{os::unix::net::UnixListener, sync::atomic::AtomicBool};

    #[test]
    fn wrong_invocation_nonce_never_establishes_cleanup_ownership() {
        let home = tempfile::tempdir().unwrap();
        let endpoint = home.path().join("collector");
        let owner = EndpointOwner::new(&endpoint, "correct-invocation").unwrap();
        let listener = UnixListener::bind(&endpoint).unwrap();
        assert!(acknowledge(
            &endpoint,
            owner.channel(),
            "different-invocation",
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(owner.verify().is_err());
        drop(owner);
        assert!(endpoint.exists(), "unauthenticated endpoint was unlinked");
        drop(listener);
    }

    #[test]
    fn unacknowledged_bind_window_is_conservatively_not_cleanup_owned() {
        let home = tempfile::tempdir().unwrap();
        let endpoint = home.path().join("collector");
        let owner = EndpointOwner::new(&endpoint, "invocation").unwrap();
        let listener = UnixListener::bind(&endpoint).unwrap();
        drop(owner);
        assert!(
            endpoint.exists(),
            "endpoint observed only after teardown was adopted"
        );
        drop(listener);
    }
}
