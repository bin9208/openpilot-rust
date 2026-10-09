//! OS-trusted RTMPS tunnel; original accept/TCP/TLS/send/join deadlines are preserved.
use super::tls_poll::readable;
use crate::Error;
use std::{
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) struct Shared {
    pub stop: AtomicBool,
    pub error: Mutex<String>,
    sockets: Mutex<Vec<TcpStream>>,
}
impl Shared {
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(sockets) = self.sockets.lock() {
            for socket in sockets.iter() {
                let _shutdown = socket.shutdown(Shutdown::Both);
            }
        }
    }
    fn retain(&self, socket: &TcpStream) -> io::Result<()> {
        let copy = socket.try_clone()?;
        let mut sockets = self
            .sockets
            .lock()
            .map_err(|_| io::Error::other("TLS socket ownership poisoned"))?;
        if self.stop.load(Ordering::SeqCst) {
            copy.shutdown(Shutdown::Both)?;
        }
        sockets.push(copy);
        Ok(())
    }
}
pub(super) struct Tunnel {
    pub port: u16,
    pub shared: Arc<Shared>,
    thread: Option<thread::JoinHandle<()>>,
    done: mpsc::Receiver<()>,
}
impl Tunnel {
    pub fn start(host: String, port: u16) -> Result<Self, Error> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port_local = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            error: Mutex::new(String::new()),
            sockets: Mutex::new(Vec::new()),
        });
        let owned = Arc::clone(&shared);
        let (complete, done) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("youtube-rtmps-tunnel".into())
            .spawn(move || {
                if let Err(error) = run(listener, (host, port), &owned) {
                    if !owned.stop.load(Ordering::SeqCst) {
                        if let Ok(mut detail) = owned.error.lock() {
                            *detail = error.to_string();
                        }
                    }
                }
                if let Ok(mut sockets) = owned.sockets.lock() {
                    for socket in sockets.drain(..) {
                        let _shutdown = socket.shutdown(Shutdown::Both);
                    }
                }
                let _finished = complete.send(());
            })?;
        Ok(Self {
            port: port_local,
            shared,
            thread: Some(thread),
            done,
        })
    }
    pub fn close(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.shared.shutdown();
        if self.done.recv_timeout(Duration::from_secs(2)).is_ok() {
            let _finished = thread.join();
        }
    }
}
impl Drop for Tunnel {
    fn drop(&mut self) {
        self.close();
    }
}
fn run(listener: TcpListener, endpoint: (String, u16), shared: &Shared) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let local = loop {
        if shared.stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "timed out"));
        }
        readable(&[&listener], remaining.min(Duration::from_millis(100)))?;
    };
    shared.retain(&local)?;
    let remote = super::tls_relay::connect(&endpoint)?;
    shared.retain(&remote)?;
    super::tls_relay::relay(local, remote, endpoint.0, shared)
}
