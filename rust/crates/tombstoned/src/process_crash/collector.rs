//! Independent receive and upload owners: a blocked upload cannot delay capture.
use super::store::{self, Prepared, Store};
use openpilot_logging::{Fields, Value};
use std::{
    fs, io,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixDatagram,
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub trait Provider: Send + Sync + 'static {
    fn capture(&self, event: &Fields) -> io::Result<String>;
    fn payload(&self, event: &Fields) -> io::Result<Fields>;
    /// Must observe cancellation, including while an HTTP operation is blocked.
    fn upload(
        &self,
        target: &str,
        payload: &Fields,
        path: &Path,
        stop: &AtomicBool,
    ) -> io::Result<bool>;
}
pub struct Collector {
    stop: Arc<AtomicBool>,
    tasks: Vec<JoinHandle<()>>,
    socket: PathBuf,
    identity: (u64, u64),
}
fn mutex(store: &Mutex<Store>) -> io::Result<std::sync::MutexGuard<'_, Store>> {
    store
        .lock()
        .map_err(|_| io::Error::other("crash store poisoned"))
}
impl Collector {
    pub fn start(root: PathBuf, address: PathBuf, provider: Arc<dyn Provider>) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        match fs::remove_file(&address) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let socket = UnixDatagram::bind(&address)?;
        fs::set_permissions(&address, fs::Permissions::from_mode(0o600))?;
        socket.set_read_timeout(Some(Duration::from_millis(500)))?;
        let metadata = fs::metadata(&address)?;
        let mut owner = Self {
            stop: Arc::new(AtomicBool::new(false)),
            tasks: Vec::new(),
            socket: address,
            identity: (metadata.dev(), metadata.ino()),
        };
        let store = Arc::new(Mutex::new(Store::new(root)));
        let stop = Arc::clone(&owner.stop);
        let receiver_store = Arc::clone(&store);
        let receiver_provider = Arc::clone(&provider);
        owner.tasks.push(
            thread::Builder::new()
                .name("crash-capture".into())
                .spawn(move || {
                    let mut bytes = [0; 8192];
                    while !stop.load(Ordering::Relaxed) {
                        let count = match socket.recv(&mut bytes) {
                            Ok(n) => n,
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    io::ErrorKind::WouldBlock
                                        | io::ErrorKind::TimedOut
                                        | io::ErrorKind::Interrupted
                                ) =>
                            {
                                continue
                            }
                            Err(e) => {
                                if !stop.load(Ordering::Relaxed) {
                                    eprintln!("[process_crash] receive: {e}");
                                }
                                continue;
                            }
                        };
                        let result = (|| {
                            let event = store::fields(&bytes[..count])?;
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map_err(io::Error::other)?
                                .as_secs_f64();
                            let prepared = Prepared::capture(
                                event,
                                now,
                                |event| receiver_provider.capture(event),
                                |event| receiver_provider.payload(event),
                            )?;
                            mutex(&receiver_store)?.record(prepared)
                        })();
                        if let Err(error) = result {
                            eprintln!("[process_crash] capture failed: {error}");
                        }
                    }
                })?,
        );
        let stop = Arc::clone(&owner.stop);
        owner.tasks.push(
            thread::Builder::new()
                .name("crash-upload".into())
                .spawn(move || {
                    let started = Instant::now();
                    while !stop.load(Ordering::Relaxed) {
                        if let Err(error) = upload_pending(
                            &store,
                            &*provider,
                            started.elapsed().as_secs_f64(),
                            &stop,
                        ) {
                            eprintln!("[process_crash] upload retry: {error}");
                        }
                        let until = Instant::now() + Duration::from_secs(1);
                        while !stop.load(Ordering::Relaxed) && Instant::now() < until {
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                })?,
        );
        Ok(owner)
    }
    pub fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for task in self.tasks.drain(..) {
            if task.join().is_err() {
                eprintln!("[process_crash] worker panicked");
            }
        }
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        self.close();
        if fs::metadata(&self.socket).is_ok_and(|m| (m.dev(), m.ino()) == self.identity) {
            let _ = fs::remove_file(&self.socket);
        }
    }
}
fn upload_pending(
    store: &Mutex<Store>,
    provider: &dyn Provider,
    now: f64,
    stop: &AtomicBool,
) -> io::Result<()> {
    let paths = mutex(store)?.due(now)?;
    for path in paths {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let result = (|| {
            let report = {
                let _guard = mutex(store)?;
                store::fields(&fs::read(&path)?)?
            };
            let Some(Value::Object(payload)) = report.get("payload") else {
                return Err(io::Error::other("invalid crash payload"));
            };
            for target in ["dsm", "carrot_logs"] {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if mutex(store)?.acknowledged(&path, target)? {
                    continue;
                }
                if provider
                    .upload(target, payload, &path.with_extension("txt"), stop)
                    .unwrap_or(false)
                {
                    mutex(store)?.acknowledge(&path, target)?;
                }
            }
            Ok::<_, io::Error>(())
        })();
        if let Err(error) = result {
            eprintln!("[process_crash] pending report: {error}");
        }
        mutex(store)?.defer(&path, now);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Blocked {
        entered: AtomicBool,
        release: AtomicBool,
    }
    impl Provider for Blocked {
        fn capture(&self, _: &Fields) -> io::Result<String> {
            Ok("original traceback".into())
        }
        fn payload(&self, _: &Fields) -> io::Result<Fields> {
            Ok([("tmux_why".into(), Value::Text("process_crash".into()))]
                .into_iter()
                .collect())
        }
        fn upload(&self, _: &str, _: &Fields, _: &Path, stop: &AtomicBool) -> io::Result<bool> {
            self.entered.store(true, Ordering::Relaxed);
            while !self.release.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(false)
        }
    }
    fn until(mut test: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !test() {
            assert!(Instant::now() < deadline, "collector deadline");
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn real_socket_captures_while_upload_is_blocked_and_joins_on_cancel() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("reports");
        let socket = directory.path().join("socket");
        let provider = Arc::new(Blocked {
            entered: AtomicBool::new(false),
            release: AtomicBool::new(false),
        });
        let mut collector =
            Collector::start(root.clone(), socket.clone(), provider.clone()).unwrap();
        assert_eq!(
            fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let sender = UnixDatagram::unbound().unwrap();
        sender.set_nonblocking(true).unwrap();
        sender
            .send_to(
                br#"{"id":"1-101-1","name":"worker","pid":101,"exitcode":-6}"#,
                &socket,
            )
            .unwrap();
        until(|| provider.entered.load(Ordering::Relaxed));
        let started = Instant::now();
        sender
            .send_to(
                br#"{"id":"1-102-2","name":"worker","pid":102,"exitcode":-9}"#,
                &socket,
            )
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
        until(|| root.join("1-102-2.json").is_file());
        for (name, signal) in [("1-101-1", "SIGABRT"), ("1-102-2", "SIGKILL")] {
            let report =
                store::fields(&fs::read(root.join(format!("{name}.json"))).unwrap()).unwrap();
            let Value::Object(event) = &report["event"] else {
                panic!("event")
            };
            assert_eq!(store::text(event, "signal"), Some(signal));
        }
        let started = Instant::now();
        collector.close();
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(collector);
        assert!(!socket.exists());
    }
}
