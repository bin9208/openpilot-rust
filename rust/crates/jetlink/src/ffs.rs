//! FunctionFS v2 transport, preserving the pinned Jetlink descriptor/chunk contract.
//! Derived from third_party/jetlink/transport/ffs.py, MIT (Zeph Leggett, 2026).
use crate::{
    client::{Message, Transport},
    platform,
    wire::{self, Header},
    Deadline, Error,
};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TrySendError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
const CHUNK: usize = 16 * 1024;
const MAX_MESSAGE: usize = 16 << 20;
pub struct Session {
    ep0: Mutex<Option<File>>,
    usb_role: Mutex<()>,
    gadget: PathBuf,
    mount: PathBuf,
    state: File,
    speed: PathBuf,
    stopped: AtomicBool,
    host_ready_until: Mutex<Option<Instant>>,
}
impl Session {
    /// Caller must have exclusively provisioned this gadget.
    pub fn open(mount: &Path, gadget: &Path, udc_root: &Path) -> Result<Arc<Self>, Error> {
        let mut ep0 = OpenOptions::new()
            .read(true)
            .write(true)
            .open(mount.join("ep0"))?;
        let descriptor = &mut ep0;
        descriptor.write_all(&wire::descriptors())?;
        descriptor.write_all(&wire::strings())?;
        let mut controllers = fs::read_dir(udc_root)?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<Result<Vec<_>, _>>()?;
        controllers.sort();
        let controller = controllers
            .first()
            .ok_or(Error::Contract("no USB controller"))?;
        let state = File::open(udc_root.join(controller).join("state"))?;
        fs::write(
            gadget.join("UDC"),
            format!("{}\n", controller.to_string_lossy()),
        )?;
        Ok(Arc::new(Self {
            ep0: Mutex::new(Some(ep0)),
            usb_role: Mutex::new(()),
            gadget: gadget.to_owned(),
            mount: mount.to_owned(),
            state,
            speed: udc_root.join(controller).join("current_speed"),
            stopped: AtomicBool::new(false),
            host_ready_until: Mutex::new(None),
        }))
    }
    pub fn speed(&self) -> Result<String, Error> {
        Ok(fs::read_to_string(&self.speed)?.trim().to_owned())
    }
    pub fn bound(&self) -> Result<bool, Error> {
        Ok(!self.stopped.load(Ordering::Acquire)
            && !fs::read_to_string(self.gadget.join("UDC"))?
                .trim()
                .is_empty())
    }
    pub fn configured(&self) -> Result<bool, Error> {
        let mut buffer = [0; 64];
        let n = self.state.read_at(&mut buffer, 0)?;
        Ok(buffer[..n].trim_ascii() == b"configured")
    }
    fn wait_configured(&self, deadline: Deadline) -> Result<(), Error> {
        while !self.configured()? {
            if self.stopped.load(Ordering::Acquire) {
                return Err(Error::Closed);
            }
            deadline.remaining()?;
            thread::park_timeout(Duration::from_millis(20));
        }
        Ok(())
    }
    fn retry_host_claim(&self) -> Result<bool, Error> {
        if self.stopped.load(Ordering::Acquire) || !self.configured()? {
            return Ok(false);
        }
        let mut until = self
            .host_ready_until
            .lock()
            .map_err(|_| Error::Contract("host readiness poisoned"))?;
        let deadline = *until.get_or_insert_with(|| Instant::now() + Duration::from_secs(10));
        if Instant::now() >= deadline {
            return Ok(false);
        }
        drop(until);
        thread::park_timeout(Duration::from_millis(5));
        Ok(true)
    }
    pub fn teardown(&self) -> Result<(), Error> {
        let _role = self
            .usb_role
            .lock()
            .map_err(|_| Error::Contract("USB role mutex poisoned"))?;
        self.stopped.store(true, Ordering::Release);
        fs::write(self.gadget.join("UDC"), b"\n")?;
        self.ep0
            .lock()
            .map_err(|_| Error::Contract("ep0 mutex poisoned"))?
            .take();
        Ok(())
    }
    fn abort_write(&self) -> Result<(), Error> {
        let _role = self
            .usb_role
            .lock()
            .map_err(|_| Error::Contract("USB role mutex poisoned"))?;
        if self.stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        fs::write(self.gadget.join("UDC"), b"\n")?;
        Ok(())
    }
}

struct Watch {
    deadline: Mutex<Option<Deadline>>,
    aborted: AtomicBool,
    stopped: AtomicBool,
}
pub struct FfsTransport {
    version: wire::Version,
    session: Arc<Session>,
    writer: Option<File>,
    chunks: Option<Receiver<Result<Vec<u8>, Error>>>,
    free: Arc<Mutex<Vec<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
    watch: Arc<Watch>,
    receive: VecDeque<u8>,
}
impl FfsTransport {
    pub fn new(session: Arc<Session>) -> Self {
        Self {
            session,
            version: wire::Version::V2,
            writer: None,
            chunks: None,
            free: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
            watch: Arc::new(Watch {
                deadline: Mutex::new(None),
                aborted: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
            }),
            receive: VecDeque::with_capacity(256 << 10),
        }
    }
    pub fn with_version(session: Arc<Session>, version: wire::Version) -> Self {
        let mut transport = Self::new(session);
        transport.version = version;
        transport
    }
    fn endpoints(&mut self, deadline: Deadline) -> Result<(), Error> {
        if self.writer.is_some() {
            return Ok(());
        }
        let bound = Deadline::after(Duration::from_secs(10))?;
        self.session
            .wait_configured(Deadline(bound.0.min(deadline.0)))?;
        let reader = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.session.mount.join("ep1"))?;
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.session.mount.join("ep2"))?;
        let (tx, rx) = mpsc::sync_channel(512);
        let stop = Arc::clone(&self.stop);
        let session = Arc::clone(&self.session);
        let free = Arc::clone(&self.free);
        thread::Builder::new()
            .name("jetlink-ffs-read".into())
            .spawn(move || {
                let result = read_loop(reader, &session, &stop, &free, &tx);
                if let Err(error) = result {
                    if tx.send(Err(error)).is_err() {
                        eprintln!("jetlink reader receiver closed");
                    }
                }
            })?;
        let watch = Arc::clone(&self.watch);
        let session = Arc::clone(&self.session);
        thread::Builder::new()
            .name("jetlink-ffs-watch".into())
            .spawn(move || {
                platform::background();
                while !watch.stopped.load(Ordering::Acquire) {
                    let expired = match watch.deadline.lock() {
                        Ok(value) => value.is_some_and(|deadline| deadline.remaining().is_err()),
                        Err(_) => true,
                    };
                    if expired {
                        watch.aborted.store(true, Ordering::Release);
                        if let Err(error) = session.abort_write() {
                            eprintln!("jetlink write watchdog: {error}");
                        }
                        break;
                    }
                    thread::park_timeout(Duration::from_millis(2));
                }
            })?;
        self.writer = Some(writer);
        self.chunks = Some(rx);
        Ok(())
    }
    fn fill(&mut self, count: usize, deadline: Deadline) -> Result<(), Error> {
        while self.receive.len() < count {
            let mut chunk = self
                .chunks
                .as_ref()
                .ok_or(Error::Closed)?
                .recv_timeout(deadline.remaining()?)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => Error::Deadline,
                    mpsc::RecvTimeoutError::Disconnected => Error::Closed,
                })??;
            self.receive.extend(chunk.iter().copied());
            chunk.clear();
            let mut free = self
                .free
                .lock()
                .map_err(|_| Error::Contract("reader buffers poisoned"))?;
            if free.len() < 16 {
                free.push(chunk);
            }
        }
        Ok(())
    }
}
fn read_loop(
    mut reader: File,
    session: &Session,
    stop: &AtomicBool,
    free: &Mutex<Vec<Vec<u8>>>,
    tx: &mpsc::SyncSender<Result<Vec<u8>, Error>>,
) -> Result<(), Error> {
    let _mask = platform::SignalMask::block_all()?;
    platform::background();
    if let Err(error) = platform::scheduler(Some(51)) {
        eprintln!("jetlink reader priority: {error}");
    }
    {
        let mut pool = free
            .lock()
            .map_err(|_| Error::Contract("reader buffers poisoned"))?;
        for _ in 0..16 {
            pool.push(Vec::with_capacity(CHUNK));
        }
    }
    while !stop.load(Ordering::Acquire) && !session.stopped.load(Ordering::Acquire) {
        if !session.configured()? {
            return Err(Error::Contract("host disconnected"));
        }
        let mut buffer = free
            .lock()
            .map_err(|_| Error::Contract("reader buffers poisoned"))?
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(CHUNK));
        buffer.resize(CHUNK, 0);
        let size = match reader.read(&mut buffer) {
            Ok(size) => size,
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(libc::EIO | libc::ESHUTDOWN | libc::ENODEV)
                ) && session.retry_host_claim()? =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        if size == 0 {
            return Err(Error::Closed);
        }
        buffer.truncate(size);
        let mut item = Ok(buffer);
        loop {
            match tx.try_send(item) {
                Ok(()) => break,
                Err(TrySendError::Full(returned)) => {
                    item = returned;
                    if stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    thread::park_timeout(Duration::from_millis(2));
                }
                Err(TrySendError::Disconnected(_)) => return Ok(()),
            }
        }
    }
    Ok(())
}
impl Transport for FfsTransport {
    fn version(&self) -> wire::Version {
        self.version
    }
    fn send(&mut self, mut bytes: &[u8], deadline: Deadline) -> Result<(), Error> {
        self.endpoints(deadline)?;
        if self.watch.aborted.load(Ordering::Acquire) {
            return Err(Error::Closed);
        }
        let _mask = platform::SignalMask::block_all()?;
        *self
            .watch
            .deadline
            .lock()
            .map_err(|_| Error::Contract("watchdog poisoned"))? = Some(deadline);
        let result = (|| {
            let mut quantum = 512 * 1024;
            while !bytes.is_empty() {
                deadline.remaining()?;
                let n = match self
                    .writer
                    .as_mut()
                    .ok_or(Error::Closed)?
                    .write(&bytes[..bytes.len().min(quantum)])
                {
                    Ok(0) => return Err(Error::Closed),
                    Ok(n) => n,
                    Err(error) if error.raw_os_error() == Some(libc::ENOMEM) && quantum > CHUNK => {
                        quantum = (quantum / 2).max(CHUNK);
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                bytes = &bytes[n..];
            }
            deadline.remaining()?;
            Ok(())
        })();
        *self
            .watch
            .deadline
            .lock()
            .map_err(|_| Error::Contract("watchdog poisoned"))? = None;
        if self.watch.aborted.load(Ordering::Acquire) {
            return Err(Error::Deadline);
        }
        result
    }
    fn receive(&mut self, deadline: Deadline) -> Result<Message, Error> {
        self.endpoints(deadline)?;
        self.fill(32, deadline)?;
        let mut bytes = [0; 32];
        for (target, source) in bytes.iter_mut().zip(self.receive.iter()) {
            *target = *source;
        }
        let header = Header::decode_version(&bytes, self.version)?;
        let length = usize::try_from(header.length).map_err(|_| Error::Contract("wire length"))?;
        if length > MAX_MESSAGE {
            return Err(Error::Contract("wire message cap"));
        }
        let pad = usize::from(header.flags & wire::PADDED != 0);
        self.fill(32 + length + pad, deadline)?;
        self.receive.drain(..32);
        let payload = self.receive.drain(..length).collect();
        self.receive.drain(..pad);
        deadline.remaining()?;
        Ok(Message { header, payload })
    }
    fn close(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.watch.stopped.store(true, Ordering::Release);
        if !self.session.stopped.load(Ordering::Acquire) {
            if let Err(error) = self.session.teardown() {
                eprintln!("jetlink transport close: {error}");
            }
        }
    }
}
impl Drop for FfsTransport {
    fn drop(&mut self) {
        self.close();
    }
}
