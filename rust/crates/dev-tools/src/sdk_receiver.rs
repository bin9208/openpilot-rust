//! Loopback Sentry envelope capture for native development checks.
//! Payload decoding follows the retained source receiver; socket lifetime is
//! explicitly owned so partial requests cannot outlive a failed comparison.
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub struct Captured {
    pub event: JsonValue,
    pub header: JsonValue,
    pub raw: String,
    pub wire: Vec<u8>,
    pub path: String,
    pub auth: Option<String>,
    pub content_encoding: Option<String>,
    pub headers: BTreeMap<String, String>,
}
impl std::fmt::Debug for Captured {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Captured")
            .field("path", &self.path)
            .field("content_encoding", &self.content_encoding)
            .finish_non_exhaustive()
    }
}
pub fn decode_envelope(wire: &[u8], encoding: Option<&str>) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    match encoding {
        None | Some("identity") => return Ok(wire.to_vec()),
        Some("gzip") => {
            flate2::read::MultiGzDecoder::new(wire).read_to_end(&mut output)?;
        }
        Some("br") => {
            brotli_decompressor::Decompressor::new(wire, 4096).read_to_end(&mut output)?;
        }
        Some(value) => return Err(format!("unsupported envelope Content-Encoding: {value}").into()),
    }
    Ok(output)
}
struct OwnedSockets {
    stopping: AtomicBool,
    clients: Mutex<BTreeMap<u64, TcpStream>>,
}
/// Each accepted client's I/O has a total deadline, independent of other clients.
/// `close` and Drop shut down accepted sockets and join every owned thread.
pub struct Receiver {
    address: SocketAddr,
    sockets: Arc<OwnedSockets>,
    events: mpsc::Receiver<Result<Captured>>,
    owner: Option<JoinHandle<Result<()>>>,
}
impl Receiver {
    pub fn bind(request_timeout: Duration) -> Result<Self> {
        if request_timeout.is_zero() {
            return Err("request deadline must be positive".into());
        }
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let sockets = Arc::new(OwnedSockets {
            stopping: AtomicBool::new(false),
            clients: Mutex::new(BTreeMap::new()),
        });
        let owned = Arc::clone(&sockets);
        let (send, events) = mpsc::channel();
        let owner = thread::Builder::new()
            .name("sdk-receiver".into())
            .spawn(move || {
                let mut workers = Vec::new();
                let mut next = 0u64;
                let mut failure = None;
                while !owned.stopping.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let Some(deadline) = Instant::now().checked_add(request_timeout) else {
                                failure = Some("request deadline overflow".into());
                                break;
                            };
                            let clone = match stream.try_clone() {
                                Ok(clone) => clone,
                                Err(error) => {
                                    failure = Some(error.into());
                                    break;
                                }
                            };
                            let id = next;
                            next += 1;
                            {
                                let mut clients = owned.clients.lock().unwrap();
                                if owned.stopping.load(Ordering::Acquire) {
                                    break;
                                }
                                clients.insert(id, clone);
                            }
                            let clients = Arc::clone(&owned);
                            let send = send.clone();
                            match thread::Builder::new().name("sdk-envelope".into()).spawn(
                                move || {
                                    let result = response(stream, deadline, &send);
                                    clients.clients.lock().unwrap().remove(&id);
                                    if let Err(error) = result {
                                        if !clients.stopping.load(Ordering::Acquire) {
                                            let _ = send.send(Err(error));
                                        }
                                    }
                                },
                            ) {
                                Ok(worker) => workers.push(worker),
                                Err(error) => {
                                    owned.clients.lock().unwrap().remove(&id);
                                    failure = Some(error.into());
                                    break;
                                }
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => {
                            failure = Some(error.into());
                            break;
                        }
                    }
                }
                // Also drain sockets on an accept/spawn failure, not only user close.
                owned.stopping.store(true, Ordering::Release);
                for stream in owned.clients.lock().unwrap().values() {
                    let _ = stream.shutdown(Shutdown::Both);
                }
                for worker in workers {
                    if worker.join().is_err() && failure.is_none() {
                        failure = Some("envelope worker panicked".into());
                    }
                }
                if let Some(error) = failure {
                    Err(error)
                } else {
                    Ok(())
                }
            })?;
        Ok(Self {
            address,
            sockets,
            events,
            owner: Some(owner),
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn dsn(&self) -> String {
        format!("http://fixture@{}/1", self.address)
    }
    pub fn receive(&self, timeout: Duration) -> Result<Captured> {
        self.events.recv_timeout(timeout)?
    }
    pub fn require_empty(&self) -> Result<()> {
        match self.events.try_recv() {
            Err(mpsc::TryRecvError::Empty) => Ok(()),
            Err(error) => Err(error.into()),
            Ok(_) => Err("unexpected extra envelope event".into()),
        }
    }
    pub fn close(&mut self) -> Result<()> {
        self.sockets.stopping.store(true, Ordering::Release);
        for stream in self.sockets.clients.lock().unwrap().values() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        match self.owner.take() {
            Some(owner) => owner.join().map_err(|_| "receiver owner panicked")?,
            None => Ok(()),
        }
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
struct DeadlineStream {
    stream: TcpStream,
    deadline: Instant,
}
impl Read for DeadlineStream {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "request deadline elapsed"))?;
        self.stream.set_read_timeout(Some(remaining))?;
        match self.stream.read(output) {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "request deadline elapsed",
                ))
            }
            result => result,
        }
    }
}
fn line(reader: &mut BufReader<DeadlineStream>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    // The original HTTP parser also limits each request/header line to 65536.
    reader.take(65537).read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Err("unexpected HTTP EOF".into());
    }
    if bytes.len() > 65536 {
        return Err("HTTP line too long".into());
    }
    Ok(bytes)
}
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| char::from(*b)).collect()
}
fn parse_json(bytes: &[u8]) -> Result<JsonValue> {
    Ok(JsonValue::parse(std::str::from_utf8(bytes)?)?)
}
fn payload_length(value: &JsonValue, available: usize) -> Result<usize> {
    match value.view() {
        JsonView::Bool(value) => Ok(usize::from(value).min(available)),
        JsonView::Integer(text) => {
            if let Some(magnitude) = text.strip_prefix('-') {
                Ok(available.saturating_sub(magnitude.parse::<usize>().unwrap_or(usize::MAX)))
            } else {
                Ok(text.parse::<usize>().unwrap_or(usize::MAX).min(available))
            }
        }
        _ => Err("envelope length is not an integer".into()),
    }
}
fn response(
    stream: TcpStream,
    deadline: Instant,
    send: &mpsc::Sender<Result<Captured>>,
) -> Result<()> {
    let mut reader = BufReader::new(DeadlineStream { stream, deadline });
    let request = latin1(&line(&mut reader)?);
    let fields = request.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 || fields[0] != "POST" || fields[1] != "/api/1/envelope/" {
        return Err("unexpected envelope endpoint".into());
    }
    let path = fields[1].to_owned();
    let mut headers = BTreeMap::new();
    loop {
        let bytes = line(&mut reader)?;
        if bytes == b"\r\n" || bytes == b"\n" {
            break;
        }
        if headers.len() >= 100 {
            return Err("too many HTTP headers".into());
        }
        let value = latin1(&bytes);
        let (key, value) = value.split_once(':').ok_or("invalid HTTP header")?;
        headers.insert(key.to_owned(), value.trim().to_owned());
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    };
    let length = header("Content-Length")
        .ok_or("missing Content-Length")?
        .parse::<usize>()?;
    let encoding = header("Content-Encoding");
    let auth = header("X-Sentry-Auth");
    let mut wire = Vec::new();
    (&mut reader).take(length as u64).read_to_end(&mut wire)?;
    if wire.len() != length {
        return Err("incomplete envelope body".into());
    }
    let body = decode_envelope(&wire, encoding.as_deref())?;
    let mut parts = body.splitn(3, |byte| *byte == b'\n');
    let envelope = parse_json(parts.next().ok_or("missing envelope header")?)?;
    let item = parse_json(parts.next().ok_or("missing envelope item")?)?;
    if !item.get("type").is_some_and(|kind| kind.text_eq("event")) {
        return Err("not an event envelope".into());
    }
    let payload = parts.next().ok_or("missing event payload")?;
    let length = payload_length(
        &item.get("length").ok_or("missing event length")?,
        payload.len(),
    )?;
    let event = parse_json(&payload[..length])?;
    let raw = String::from_utf8(body)?;
    // The original handler publishes before acknowledging. A peer resetting
    // its connection after a valid body must not retract the captured event.
    send.send(Ok(Captured {
        event,
        header: envelope,
        raw,
        wire,
        path,
        auth,
        content_encoding: encoding,
        headers,
    }))?;
    if let Some(remaining) = deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
    {
        let stream = &mut reader.get_mut().stream;
        if let Err(error) = stream
            .set_write_timeout(Some(remaining))
            .and_then(|()| stream.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\n{}"))
        {
            eprintln!("Sentry fixture acknowledgement: {error}");
        }
    }
    Ok(())
}
