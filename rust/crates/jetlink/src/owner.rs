use crate::{
    contract::{self, Identity},
    rpc, Deadline, Error,
};
use std::{
    fs::{self, File},
    io::Read,
    net::Shutdown,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub trait Backend: Send + 'static {
    fn connect(&mut self) -> Result<Identity, Error>;
    fn infer(
        &mut self,
        frame: u32,
        warped: &[u8],
        packed: &[f32],
        deadline: Deadline,
        reset: bool,
    ) -> Result<Vec<f32>, Error>;
    fn dead(&self) -> bool;
    fn close(&mut self);
}
#[derive(Default)]
pub struct GadgetOwner {
    enabled: bool,
}
impl GadgetOwner {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn enable(
        &mut self,
        offroad: bool,
        egpu_present: bool,
        setup: impl FnOnce() -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.enabled {
            return Ok(());
        }
        if !offroad {
            return Err(Error::Contract("USB setup requires offroad"));
        }
        if egpu_present {
            return Err(Error::Contract("eGPU owns USB"));
        }
        setup()?;
        self.enabled = true;
        Ok(())
    }
    pub fn disable(
        &mut self,
        offroad: bool,
        teardown: impl FnOnce() -> Result<(), Error>,
    ) -> Result<(), Error> {
        if !self.enabled {
            return Ok(());
        }
        if !offroad {
            return Err(Error::Contract("USB teardown requires offroad"));
        }
        teardown()?;
        self.enabled = false;
        Ok(())
    }
}

pub fn infer_request(
    backend: &mut impl Backend,
    generation: [u8; 16],
    payload: &[u8],
) -> Result<(Vec<u8>, Deadline), Error> {
    if payload.len()
        != 1 + rpc::REQUEST_BYTES + contract::WARPED_BYTES + contract::PACKED_FLOATS * 4
        || payload[0] != b'I'
    {
        return Err(Error::Contract("inference request"));
    }
    if payload[1..17] != generation || payload[29] > 1 {
        return Err(Error::Contract("request identity/reset"));
    }
    let frame = u32::from_le_bytes(
        payload[17..21]
            .try_into()
            .map_err(|_| Error::Contract("frame"))?,
    );
    let deadline = Deadline(u64::from_le_bytes(
        payload[21..29]
            .try_into()
            .map_err(|_| Error::Contract("deadline"))?,
    ));
    if deadline.remaining()? > Duration::from_millis(50) {
        return Err(Error::Contract("frame budget exceeds 50ms"));
    }
    let offset = 1 + rpc::REQUEST_BYTES;
    let warped = &payload[offset..offset + contract::WARPED_BYTES];
    let packed = rpc::decode_floats(&payload[offset + contract::WARPED_BYTES..])?;
    contract::check_input(warped, &packed)?;
    let output = backend.infer(frame, warped, &packed, deadline, payload[29] == 1)?;
    contract::check_output(&output)?;
    deadline.remaining()?;
    let mut response = Vec::with_capacity(21 + output.len() * 4);
    response.push(b'R');
    response.extend(generation);
    response.extend(frame.to_le_bytes());
    for value in output {
        response.extend(value.to_le_bytes());
    }
    Ok((response, deadline))
}

pub struct Server {
    stop: Arc<AtomicBool>,
    connection: Arc<Mutex<Option<UnixStream>>>,
    done: mpsc::Receiver<Result<(), Error>>,
    worker: Option<JoinHandle<()>>,
    pub ready: Arc<AtomicBool>,
    pub error: Arc<Mutex<String>>,
    pub generation: [u8; 16],
    path: PathBuf,
    owned_socket: Arc<Mutex<Option<File>>>,
    stop_invoked: bool,
}
impl Server {
    /// Starts only the local worker. The caller owns gadget setup and the process lock.
    pub fn spawn(mut backend: impl Backend, path: &Path) -> Result<Self, Error> {
        let mut generation = [0; 16];
        fs::File::open("/dev/urandom")?.read_exact(&mut generation)?;
        let stop = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(String::new()));
        let worker_error = Arc::clone(&error);
        let connection = Arc::new(Mutex::new(None));
        let owned_socket = Arc::new(Mutex::new(None));
        let (done_tx, done) = mpsc::channel();
        let worker_stop = Arc::clone(&stop);
        let worker_ready = Arc::clone(&ready);
        let worker_connection = Arc::clone(&connection);
        let worker_owned_socket = Arc::clone(&owned_socket);
        let worker_path = path.to_owned();
        let worker = thread::Builder::new()
            .name("jetlink-usb-owner".into())
            .spawn(move || {
                let result = serve(
                    &mut backend,
                    &worker_path,
                    generation,
                    &worker_stop,
                    &worker_ready,
                    &worker_connection,
                    &worker_owned_socket,
                );
                if let Err(error) = &result {
                    if let Ok(mut slot) = worker_error.lock() {
                        *slot = error.to_string().chars().take(240).collect();
                    }
                }
                worker_ready.store(false, Ordering::Release);
                backend.close();
                let _ = done_tx.send(result);
            })?;
        Ok(Self {
            stop,
            connection,
            done,
            worker: Some(worker),
            ready,
            error,
            generation,
            path: path.to_owned(),
            owned_socket,
            stop_invoked: false,
        })
    }
    /// A kernel-stuck USB worker is detached after the bound; it never blocks modeld shutdown.
    pub fn stop(&mut self, timeout: Duration) -> Result<(), Error> {
        self.stop_invoked = true;
        self.stop.store(true, Ordering::Release);
        if let Some(connection) = self
            .connection
            .lock()
            .map_err(|_| Error::Contract("connection mutex poisoned"))?
            .take()
        {
            if let Err(error) = connection.shutdown(Shutdown::Both) {
                if error.kind() != std::io::ErrorKind::NotConnected {
                    return Err(error.into());
                }
            }
        }
        if self.worker.is_none() {
            return Ok(());
        }
        // The worker already recorded its error in Server.error.
        let _worker_result = self
            .done
            .recv_timeout(timeout)
            .map_err(|_| Error::Deadline)?;
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::Contract("owner worker panicked"))?;
        }
        let owned = self
            .owned_socket
            .lock()
            .map_err(|_| Error::Contract("socket ownership mutex poisoned"))?;
        if let Some(owned) = owned.as_ref() {
            let identity = owned.metadata()?;
            // The caller's process lock/trusted socket directory also excludes
            // replacement between this pathname check and remove_file.
            match fs::symlink_metadata(&self.path) {
                Ok(current)
                    if current.file_type().is_socket()
                        && (current.dev(), current.ino()) == (identity.dev(), identity.ino()) =>
                {
                    fs::remove_file(&self.path)?;
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if !self.stop_invoked {
            let _ = self.stop(Duration::from_secs(2));
        }
    }
}
fn serve(
    backend: &mut impl Backend,
    path: &Path,
    generation: [u8; 16],
    stop: &AtomicBool,
    ready: &AtomicBool,
    connection: &Mutex<Option<UnixStream>>,
    owned_socket: &Mutex<Option<File>>,
) -> Result<(), Error> {
    let identity = backend.connect()?;
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    // Never unlink a potentially live owner; the process-lock holder removes its stale socket before spawn.
    let listener = UnixListener::bind(path)?;
    // The process lock excludes another legitimate binder while we capture the
    // pathname. O_PATH pins its filesystem inode (the listener fd has a different
    // sockfs inode), so teardown can detect later replacement. This is not an
    // atomic defense against a concurrent actor replacing the path during bind/open.
    let owned = File::from(
        rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    if !owned.metadata()?.file_type().is_socket() {
        return Err(Error::Contract("bound socket pathname replaced"));
    }
    *owned_socket
        .lock()
        .map_err(|_| Error::Contract("socket ownership mutex poisoned"))? = Some(owned);
    // This pathname chmod assumes the same process lock/trusted directory.
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    ready.store(true, Ordering::Release);
    while !stop.load(Ordering::Acquire) {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::park_timeout(Duration::from_millis(5));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        *connection
            .lock()
            .map_err(|_| Error::Contract("connection mutex poisoned"))? = Some(stream.try_clone()?);
        let hello = rpc::recv_packet(&mut stream, Deadline::after(Duration::from_millis(500))?);
        if matches!(hello.as_deref(), Ok(b"H")) {
            let info = rpc::Hello {
                ready: true,
                generation: rpc::encode_generation(generation),
                spec: contract::contract()?,
                identity: identity.clone(),
            };
            let mut reply = vec![b'J'];
            reply.extend(serde_json::to_vec(&info)?);
            if rpc::send_packet(
                &mut stream,
                &reply,
                Deadline::after(Duration::from_millis(500))?,
            )
            .is_ok()
            {
                while !stop.load(Ordering::Acquire) {
                    let payload = match rpc::recv_packet(
                        &mut stream,
                        Deadline::after(Duration::from_secs(1))?,
                    ) {
                        Ok(payload) => payload,
                        Err(_) => break,
                    };
                    let (reply, deadline) = match infer_request(backend, generation, &payload) {
                        Ok(result) => result,
                        Err(error) if backend.dead() => return Err(error),
                        Err(_) => break,
                    };
                    if rpc::send_packet(&mut stream, &reply, deadline).is_err() {
                        break;
                    }
                }
            }
        }
        *connection
            .lock()
            .map_err(|_| Error::Contract("connection mutex poisoned"))? = None;
    }
    Ok(())
}
