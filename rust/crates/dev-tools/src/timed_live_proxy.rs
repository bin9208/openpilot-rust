//! Local progressing proxy from check_timed_shutdown.py. No public connection.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};
pub(super) struct Proxy {
    pub address: String,
    pub requests: Arc<Mutex<Vec<Value>>>,
    started: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    owner: Option<thread::JoinHandle<()>>,
}
impl Proxy {
    pub fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = format!("http://{}", listener.local_addr()?);
        let requests = Arc::new(Mutex::new(vec![]));
        let started = Arc::new(AtomicBool::new(false));
        let closed = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let captured = Arc::clone(&requests);
        let starting = Arc::clone(&started);
        let closing = Arc::clone(&closed);
        let stopping = Arc::clone(&stop);
        let owner = thread::spawn(move || {
            let mut peers = vec![];
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        let captured = Arc::clone(&captured);
                        let starting = Arc::clone(&starting);
                        let closing = Arc::clone(&closing);
                        let stopping = Arc::clone(&stopping);
                        peers.push(thread::spawn(move || {
                            let _ = serve(socket, &captured, &starting, &closing, &stopping);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
            for peer in peers {
                let _ = peer.join();
            }
        });
        Ok(Self {
            address,
            requests,
            started,
            closed,
            stop,
            owner: Some(owner),
        })
    }
    pub fn wait_started(&self) -> bool {
        wait_flag(&self.started, Duration::from_secs(5))
    }
    pub fn wait_closed(&self) -> bool {
        wait_flag(&self.closed, Duration::from_secs(2))
    }
}
fn wait_flag(flag: &AtomicBool, timeout: Duration) -> bool {
    let end = Instant::now() + timeout;
    while !flag.load(Ordering::SeqCst) {
        if Instant::now() >= end {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
    true
}
fn serve(
    mut socket: TcpStream,
    requests: &Mutex<Vec<Value>>,
    started: &AtomicBool,
    closed: &AtomicBool,
    stop: &AtomicBool,
) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut reader = BufReader::new(socket.try_clone()?);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 2 {
            return Err("malformed proxy request".into());
        }
        let method = fields[0].to_owned();
        let path = fields[1].to_owned();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
                break;
            }
        }
        if method == "CONNECT" {
            socket.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
            continue;
        }
        if method != "GET" {
            return Err("unexpected proxy method".into());
        }
        requests
            .lock()
            .map_err(|_| "proxy capture mutex poisoned")?
            .push(json!({"path":path,"started":monotonic()? ,"chunks":0}));
        let body = [
            b"{\"status\":\"success\",\"timezone\":\"missing\",\"extra\":\"".as_slice(),
            &[b'.'; 100],
            b"\"}",
        ]
        .concat();
        let result = (|| -> std::io::Result<()> {
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )?;
            socket.write_all(&body[..1])?;
            socket.flush()?;
            started.store(true, Ordering::SeqCst);
            for byte in &body[1..] {
                let end = Instant::now() + Duration::from_millis(100);
                while Instant::now() < end {
                    if stop.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                socket.write_all(&[*byte])?;
                socket.flush()?;
                let mut rows = requests
                    .lock()
                    .map_err(|_| std::io::Error::other("proxy capture mutex poisoned"))?;
                let chunks = rows[0]["chunks"].as_u64().unwrap_or(0);
                rows[0]["chunks"] = json!(chunks + 1);
            }
            Ok(())
        })();
        if let Err(error) = result {
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ) {
                closed.store(true, Ordering::SeqCst);
            }
            return Err(error.into());
        }
        return Ok(());
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(owner) = self.owner.take() {
            let _ = owner.join();
        }
    }
}
