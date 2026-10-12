use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("connectivity worker panicked")]
    Panicked,
}
struct State {
    connected: AtomicBool,
    recheck: AtomicBool,
    stop: Mutex<bool>,
    wake: Condvar,
}
pub struct Monitor {
    state: Arc<State>,
    worker: Option<JoinHandle<()>>,
}
impl Monitor {
    pub fn start(urls: Vec<String>, launcher: std::path::PathBuf) -> Result<Self, Error> {
        let state = Arc::new(State {
            connected: AtomicBool::new(false),
            recheck: AtomicBool::new(false),
            stop: Mutex::new(false),
            wake: Condvar::new(),
        });
        let shared = state.clone();
        let worker = thread::Builder::new()
            .name("agnos-network".into())
            .spawn(move || run(shared, urls, launcher))?;
        Ok(Self {
            state,
            worker: Some(worker),
        })
    }
    pub fn connected(&self) -> bool {
        self.state.connected.load(Ordering::SeqCst) && !self.state.recheck.load(Ordering::SeqCst)
    }
    pub fn invalidate(&self) {
        self.state.recheck.store(true, Ordering::SeqCst);
        self.state.connected.store(false, Ordering::SeqCst);
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        *self
            .state
            .stop
            .lock()
            .map_err(|_| std::io::Error::other("monitor state poisoned"))? = true;
        self.state.wake.notify_all();
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| Error::Panicked)?;
        }
        Ok(())
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            eprintln!("updater monitor stop: {e}");
        }
    }
}
fn run(state: Arc<State>, mut urls: Vec<String>, launcher: std::path::PathBuf) {
    if urls.is_empty() {
        urls.push("https://openpilot.comma.ai".into());
    }
    let roots: ureq::tls::RootCerts = rustls_native_certs::load_native_certs()
        .certs
        .iter()
        .map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned())
        .collect::<Vec<_>>()
        .into();
    let settings = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(roots).build())
        .build();
    let agent = ureq::Agent::new_with_config(settings);
    let mut last_restart = Duration::ZERO;
    loop {
        if *state.stop.lock().unwrap_or_else(|e| e.into_inner()) {
            break;
        }
        let outcome = urls.iter().try_for_each(|url| {
            agent
                .head(url)
                .header("User-Agent", "Python-urllib/3.12")
                .call()
                .map(|_| ())
        });
        match outcome {
            Ok(()) => {
                if state.recheck.swap(false, Ordering::SeqCst) {
                    continue;
                }
                state.connected.store(true, Ordering::SeqCst);
            }
            Err(error) => {
                state.connected.store(false, Ordering::SeqCst);
                let cert_failure = matches!(
                    error,
                    ureq::Error::Tls(_) | ureq::Error::Rustls(_) | ureq::Error::Io(_)
                ) && error
                    .to_string()
                    .to_ascii_lowercase()
                    .contains("certificate");
                use openpilot_timed::clock::Clock;
                let now = Duration::from_nanos(
                    openpilot_timed::clock::SystemClock.monotonic().unwrap_or(0),
                );
                if cert_failure
                    && !openpilot_timed::clock::valid(
                        &openpilot_timed::clock::SystemClock,
                        std::path::Path::new("/usr/lib/systemd/systemd"),
                    )
                    .unwrap_or(false)
                    && now.saturating_sub(last_restart) > Duration::from_secs(5)
                {
                    last_restart = now;
                    let cmd = openpilot_process_supervision::CapturedCommand {
                        launcher: launcher.clone(),
                        cwd: std::path::PathBuf::from("/"),
                        argv: vec![
                            "sudo".into(),
                            "systemctl".into(),
                            "restart".into(),
                            "systemd-timesyncd".into(),
                        ],
                    };
                    match cmd.spawn() {
                        Ok(mut child) => {
                            if let Err(e) = child.process.wait() {
                                eprintln!("timesyncd restart: {e}");
                            }
                        }
                        Err(e) => eprintln!("timesyncd restart: {e}"),
                    };
                }
            }
        }
        let lock = state.stop.lock().unwrap_or_else(|e| e.into_inner());
        let _ = state
            .wake
            .wait_timeout_while(lock, Duration::from_secs(1), |stopped| !*stopped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::Instant,
    };
    #[test]
    fn all_manifest_heads_required_and_failure_reset_is_advisory() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server_stop = stop.clone();
        let observed = requests.clone();
        let server = thread::spawn(move || {
            while !server_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut client, _)) => {
                        client
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut bytes = [0; 4096];
                        let count = client.read(&mut bytes).unwrap();
                        let text = String::from_utf8_lossy(&bytes[..count]).into_owned();
                        let status = if text.starts_with("HEAD /good ") {
                            "200 OK"
                        } else {
                            "503 Unavailable"
                        };
                        observed
                            .lock()
                            .unwrap()
                            .push(text.lines().next().unwrap().to_owned());
                        write!(
                            client,
                            "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                        .unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        });
        let mut monitor = Monitor::start(
            vec![
                format!("http://{address}/good"),
                format!("http://{address}/blocked"),
            ],
            "unused".into(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while requests.lock().unwrap().len() < 2 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!monitor.connected());
        monitor.stop().unwrap();
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
        println!("owned monitor endpoints: {:?}; blocked HEAD leaves connectivity false without controlling downloader",requests.lock().unwrap());
    }
    #[test]
    fn invalidation_discards_inflight_success_and_stop_interrupts_wait() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (arrived_tx, arrived_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for index in 0..2 {
                let (mut client, _) = listener.accept().unwrap();
                client
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0; 1024];
                let count = client.read(&mut bytes).unwrap();
                assert!(count > 0);
                arrived_tx.send(index).unwrap();
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                client
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .unwrap();
            }
        });
        let mut monitor =
            Monitor::start(vec![format!("http://{address}/asset")], "unused".into()).unwrap();
        assert_eq!(arrived_rx.recv_timeout(Duration::from_secs(3)).unwrap(), 0);
        monitor.invalidate();
        release_tx.send(()).unwrap();
        assert_eq!(arrived_rx.recv_timeout(Duration::from_secs(3)).unwrap(), 1);
        assert!(!monitor.connected());
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !monitor.connected() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let began = Instant::now();
        monitor.stop().unwrap();
        assert!(began.elapsed() < Duration::from_millis(200));
        server.join().unwrap();
        println!("owned HEAD0 invalidated discarded; HEAD1 alone sets connected; stop interrupts one-second interval in {:?}",began.elapsed());
    }
}
