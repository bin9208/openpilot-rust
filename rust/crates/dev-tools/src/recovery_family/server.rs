use super::Result;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const HEADER_LIMIT: usize = 65_536;
const BODY_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: String,
    pub extra_headers: Vec<(String, String)>,
}

impl Reply {
    pub fn json(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            body: body.into(),
            content_type: "application/json".into(),
            extra_headers: Vec::new(),
        }
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            content_type: String::new(),
            extra_headers: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub elapsed: Duration,
}

#[derive(Default)]
struct State {
    replies: VecDeque<Reply>,
    requests: Vec<CapturedRequest>,
    errors: Vec<String>,
}

/// A bounded owned loopback recipient for recovery webhook, upload, and CWP
/// scenarios. The reply script is independent input, never derived from the
/// candidate under test. Unknown extra requests are recorded as failures.
pub struct ScriptedHttpPeer {
    port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ScriptedHttpPeer {
    pub fn start(replies: Vec<Reply>) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let state = Arc::new(Mutex::new(State {
            replies: replies.into(),
            ..State::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&state);
        let stopping = Arc::clone(&stop);
        let started = Instant::now();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let result = handle(&mut stream, started, &shared);
                        if let Err(error) = result {
                            if let Ok(mut state) = shared.lock() {
                                state.errors.push(error.to_string());
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => {
                        if let Ok(mut state) = shared.lock() {
                            state.errors.push(error.to_string());
                        }
                        break;
                    }
                }
            }
        });
        Ok(Self {
            port,
            state,
            stop,
            worker: Some(worker),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn requests(&self) -> Result<Vec<CapturedRequest>> {
        let state = self
            .state
            .lock()
            .map_err(|_| "recovery peer state poisoned")?;
        if !state.errors.is_empty() {
            return Err(format!("recovery peer errors: {:?}", state.errors).into());
        }
        Ok(state.requests.clone())
    }

    pub fn pending_replies(&self) -> Result<usize> {
        let state = self
            .state
            .lock()
            .map_err(|_| "recovery peer state poisoned")?;
        Ok(state.replies.len())
    }

    pub fn wait_requests(&self, count: usize, timeout: Duration) -> Result<Vec<CapturedRequest>> {
        let deadline = Instant::now() + timeout;
        loop {
            let requests = self.requests()?;
            if requests.len() >= count {
                return Ok(requests);
            }
            if Instant::now() >= deadline {
                return Err("recovery scripted recipient request timeout".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for ScriptedHttpPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn handle(stream: &mut TcpStream, started: Instant, shared: &Arc<Mutex<State>>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(8)))?;
    stream.set_write_timeout(Some(Duration::from_secs(8)))?;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    let end = loop {
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            if index + 4 > HEADER_LIMIT {
                return Err("recovery peer request headers too large".into());
            }
            break index + 4;
        }
        if bytes.len() > HEADER_LIMIT {
            return Err("recovery peer request headers too large".into());
        }
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err("recovery peer request closed before headers".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    };
    let text = std::str::from_utf8(&bytes[..end])?;
    let mut lines = text.split("\r\n");
    let first = lines.next().ok_or("missing recovery request line")?;
    let mut parts = first.split_ascii_whitespace();
    let method = parts
        .next()
        .ok_or("missing recovery request method")?
        .to_owned();
    let path = parts
        .next()
        .ok_or("missing recovery request path")?
        .to_owned();
    if parts.next() != Some("HTTP/1.1") {
        return Err("unsupported recovery request version".into());
    }
    let mut headers = Vec::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or("invalid recovery request header")?;
        headers.push((key.to_ascii_lowercase(), value.trim_ascii().to_owned()));
    }
    let length = match headers.iter().find(|(key, _)| key == "content-length") {
        Some((_, value)) => value.parse::<usize>()?,
        None if method == "GET" => 0,
        None => return Err("recovery request lacks content-length".into()),
    };
    if length > BODY_LIMIT {
        return Err("recovery peer body limit exceeded".into());
    }
    let mut body = bytes[end..].to_vec();
    while body.len() < length {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err("recovery peer body truncated".into());
        }
        body.extend_from_slice(&chunk[..count]);
        if body.len() > BODY_LIMIT {
            return Err("recovery peer body limit exceeded".into());
        }
    }
    body.truncate(length);
    let reply = {
        let mut state = shared.lock().map_err(|_| "recovery peer state poisoned")?;
        state.requests.push(CapturedRequest {
            method,
            path,
            headers,
            body,
            elapsed: started.elapsed(),
        });
        state
            .replies
            .pop_front()
            .ok_or("unscripted recovery peer request")?
    };
    let reason = match reply.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Owned Fixture",
    };
    let content_type = if reply.content_type.is_empty() {
        String::new()
    } else {
        format!("Content-Type: {}\r\n", reply.content_type)
    };
    let mut extra_headers = String::new();
    for (name, value) in &reply.extra_headers {
        if name
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && byte != b'-')
            || value.bytes().any(|byte| byte == b'\r' || byte == b'\n')
        {
            return Err("invalid recovery peer response header".into());
        }
        extra_headers.push_str(&format!("{name}: {value}\r\n"));
    }
    let head = format!(
        "HTTP/1.1 {} {reason}\r\n{content_type}{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&reply.body)?;
    Ok(())
}
