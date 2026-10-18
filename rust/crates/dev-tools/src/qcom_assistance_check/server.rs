//! Private loopback HTTP peer; stopping the owner always joins its worker.
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
pub(super) struct Server {
    pub port: u16,
    stop: Arc<AtomicBool>,
    attempts: Arc<Mutex<Vec<String>>>,
    active: Arc<Mutex<Option<TcpStream>>>,
    worker: Option<JoinHandle<io::Result<()>>>,
    #[cfg(test)]
    accepted: Arc<AtomicBool>,
    #[cfg(test)]
    header_received: Arc<AtomicBool>,
}
impl Server {
    pub fn start(mode: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(Mutex::new(None));
        let connection = Arc::clone(&active);
        let flag = Arc::clone(&stop);
        #[cfg(test)]
        let accepted = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let accepted_marker = Arc::clone(&accepted);
        #[cfg(test)]
        let header_received = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let header_marker = Arc::clone(&header_received);
        let requests = Arc::clone(&attempts);
        let mode = mode.to_owned();
        let worker = thread::Builder::new()
            .name("qcom-assistance-http".into())
            .spawn(move || {
                while !flag.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            // Serialize registration with cancellation. Either
                            // the owner shuts this exact socket down, or a
                            // stop observed here prevents entering the handler.
                            let socket = stream.try_clone()?;
                            let mut current = connection
                                .lock()
                                .map_err(|_| io::Error::other("active socket lock poisoned"))?;
                            if flag.load(Ordering::Acquire) {
                                break;
                            }
                            *current = Some(socket);
                            drop(current);
                            #[cfg(test)]
                            accepted_marker.store(true, Ordering::Release);
                            let result = handle(
                                stream,
                                &mode,
                                &requests,
                                #[cfg(test)]
                                &header_marker,
                            );
                            connection
                                .lock()
                                .map_err(|_| io::Error::other("active socket lock poisoned"))?
                                .take();
                            if !flag.load(Ordering::Acquire) {
                                result?;
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                        Err(error) => return Err(error),
                    }
                }
                Ok(())
            })?;
        Ok(Self {
            port,
            stop,
            attempts,
            active,
            worker: Some(worker),
            #[cfg(test)]
            accepted,
            #[cfg(test)]
            header_received,
        })
    }
    fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut active) = self.active.lock() {
            if let Some(socket) = active.take() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
    }
    pub fn finish(mut self) -> io::Result<Vec<String>> {
        self.cancel();
        self.worker
            .take()
            .expect("owned HTTP worker")
            .join()
            .map_err(|_| io::Error::other("assistance HTTP worker panicked"))??;
        Ok(self
            .attempts
            .lock()
            .map_err(|_| io::Error::other("request lock poisoned"))?
            .clone())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn reject(stream: &mut TcpStream, status: &str) -> io::Result<()> {
    // Error payloads must not resemble the assistance download. Unsupported
    // methods do not dispatch original do_GET and cannot count as attempts.
    write!(stream, "HTTP/1.0 {status}\r\nContent-Length: 0\r\n\r\n")
}
fn handle(
    mut stream: TcpStream,
    mode: &str,
    attempts: &Mutex<Vec<String>>,
    #[cfg(test)] header_marker: &AtomicBool,
) -> io::Result<()> {
    // Match the original handler's unbounded individual reads and writes.
    // Owner cancellation shuts down this registered socket before joining.
    let mut input = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    // BaseHTTPRequestHandler uses readline(65537), rejecting >65536 bytes.
    (&mut input).take(65_537).read_line(&mut line)?;
    if line.len() > 65_536 {
        return reject(&mut stream, "414 Request-URI Too Long");
    }
    let is_get = line.split_whitespace().next() == Some("GET");
    let request = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::other("missing HTTP request path"))?
        .to_owned();
    // http.client._read_headers limits each line to65536 bytes and the
    // complete list to100 lines, including its terminating blank line.
    let mut headers = 0;
    loop {
        line.clear();
        let bytes = (&mut input).take(65_537).read_line(&mut line)?;
        if line.len() > 65_536 {
            return reject(&mut stream, "431 Line too long");
        }
        headers += 1;
        if headers > 100 {
            return reject(&mut stream, "431 Too many headers");
        }
        if bytes == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        // Test-only acknowledgement: a real complete header was consumed,
        // rather than merely accepting a socket before the pause starts.
        #[cfg(test)]
        header_marker.store(true, Ordering::Release);
    }
    if !is_get {
        return reject(&mut stream, "501 Unsupported method");
    }
    let count = {
        let mut requests = attempts
            .lock()
            .map_err(|_| io::Error::other("request lock poisoned"))?;
        requests.push(request);
        requests.len()
    };
    if mode == "retry" && count == 1 {
        return stream.shutdown(Shutdown::Both);
    }
    let body = if mode == "oversize" {
        vec![b'x'; 110_000]
    } else {
        super::BODY.to_vec()
    };
    let status = if mode == "http404" {
        "404 Not Found"
    } else {
        "200 OK"
    };
    // The oversize consumer intentionally closes before the response is exhausted.
    let _ = write!(
        stream,
        "HTTP/1.0 {status}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .and_then(|()| stream.write_all(&body));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Instant};

    fn exchange(server: &Server, request: &[u8]) -> Vec<u8> {
        let mut client = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client.write_all(request).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        if let Err(error) = client.read_to_end(&mut response) {
            assert!(error.kind() == io::ErrorKind::ConnectionReset && !response.is_empty());
        }
        response
    }
    #[test]
    fn original_get_only_peer_rejects_post_without_recording_download_attempt() {
        let server = Server::start("http404").unwrap();
        let response = exchange(&server, b"POST /assist HTTP/1.0\r\n\r\n");
        let attempts = server.finish().unwrap();
        assert!(
            response.starts_with(b"HTTP/1.0 501 "),
            "POST was accepted: {}",
            String::from_utf8_lossy(&response)
        );
        assert!(!response
            .windows(super::super::BODY.len())
            .any(|v| v == super::super::BODY));
        assert!(attempts.is_empty());
    }
    #[test]
    fn original_request_and_header_line_limits_keep_the_exact_boundary() {
        for (header, length, expected_status) in [
            (false, 65_536, 404),
            (false, 65_537, 414),
            (true, 65_536, 404),
            (true, 65_537, 431),
        ] {
            let server = Server::start("http404").unwrap();
            let mut request = if header {
                b"GET /assist HTTP/1.0\r\nX: ".to_vec()
            } else {
                b"GET /".to_vec()
            };
            let fixed = if header {
                5
            } else {
                b"GET / HTTP/1.0\r\n".len()
            };
            request.extend(vec![b'x'; length - fixed]);
            request.extend_from_slice(if header {
                b"\r\n\r\n"
            } else {
                b" HTTP/1.0\r\n\r\n"
            });
            let response = exchange(&server, &request);
            let attempts = server.finish().unwrap();
            assert!(
                response.starts_with(format!("HTTP/1.0 {expected_status} ").as_bytes()),
                "header={header} bytes={length}: {}",
                String::from_utf8_lossy(&response)
            );
            assert_eq!(attempts.len(), usize::from(expected_status == 404));
        }
    }
    #[test]
    fn original_header_count_includes_the_terminating_blank_line() {
        for (count, expected_status) in [(99, 404), (100, 431)] {
            let server = Server::start("http404").unwrap();
            let mut request = b"GET /assist HTTP/1.0\r\n".to_vec();
            for _ in 0..count {
                request.extend_from_slice(b"X: y\r\n");
            }
            request.extend_from_slice(b"\r\n");
            let response = exchange(&server, &request);
            let attempts = server.finish().unwrap();
            assert!(
                response.starts_with(format!("HTTP/1.0 {expected_status} ").as_bytes()),
                "headers={count}"
            );
            assert_eq!(attempts.len(), usize::from(count == 99));
        }
    }
    #[test]
    fn original_get_status_body_and_closed_first_retry_are_unchanged() {
        for (mode, status, bytes) in [
            ("http404", 404, super::super::BODY.to_vec()),
            ("oversize", 200, vec![b'x'; 110_000]),
            ("retry", 200, super::super::BODY.to_vec()),
        ] {
            let server = Server::start(mode).unwrap();
            if mode == "retry" {
                assert!(exchange(&server, b"GET /assist HTTP/1.0\r\n\r\n").is_empty());
            }
            let response = exchange(&server, b"GET /assist HTTP/1.0\r\n\r\n");
            let attempts = server.finish().unwrap();
            assert!(response.starts_with(format!("HTTP/1.0 {status} ").as_bytes()));
            let at = response.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
            assert_eq!(&response[at..], bytes);
            assert_eq!(attempts.len(), if mode == "retry" { 2 } else { 1 });
        }
    }
    #[test]
    fn original_get_allows_header_pause_beyond_one_second() {
        let server = Server::start("http404").unwrap();
        let header_received = Arc::clone(&server.header_received);
        let mut client = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .write_all(b"GET /assist HTTP/1.0\r\nX-Pause: ready\r\n")
            .unwrap();
        let barrier_deadline = Instant::now() + Duration::from_secs(1);
        while !header_received.load(Ordering::Acquire) && Instant::now() < barrier_deadline {
            thread::sleep(Duration::from_millis(1));
        }
        let handler_read_header = header_received.load(Ordering::Acquire);
        // The original handler has no per-read timeout; this is still well
        // inside the unchanged fifteen-second scenario observation deadline.
        if handler_read_header {
            thread::sleep(Duration::from_millis(1500));
        }
        let completed = client.write_all(b"X-After-Pause: ready\r\n\r\n");
        let mut response = Vec::new();
        let read = client.read_to_end(&mut response);
        // Cancel/close both owned endpoints and join the server before any
        // assertion, including the RED path where the handler already timed out.
        let _ = client.shutdown(Shutdown::Both);
        drop(client);
        let stopped = server.finish();
        assert!(
            handler_read_header,
            "handler never consumed the initial header"
        );
        assert!(
            completed.is_ok(),
            "normal delayed GET write failed: {completed:?}"
        );
        assert!(read.is_ok(), "normal delayed GET response failed: {read:?}");
        let attempts = stopped.expect("original-compatible delayed GET must finish normally");
        assert!(response.starts_with(b"HTTP/1.0 404 "));
        let body = response.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&response[body..], super::super::BODY);
        assert_eq!(attempts, ["/assist"]);
    }
    fn accepted_trickle_shutdown(drop_owner: bool) {
        let server = Server::start("http404").unwrap();
        let accepted = Arc::clone(&server.accepted);
        let mut client = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        client
            .write_all(b"GET /assist HTTP/1.0\r\nX-Slow: ")
            .unwrap();
        let cancel = client.try_clone().unwrap();
        let client_stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&client_stop);
        // Even the RED case owns a finite client. Close and join it before
        // asserting failure; never strand a request thread on a failed test.
        let producer = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !flag.load(Ordering::Acquire) && Instant::now() < deadline {
                if client.write_all(b".").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while !accepted.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        let was_accepted = accepted.load(Ordering::Acquire);
        let (sent, received) = mpsc::channel();
        let owner = thread::spawn(move || {
            if drop_owner {
                drop(server);
            } else {
                let _ = server.finish();
            }
            let _ = sent.send(());
        });
        let returned_while_client_alive = received.recv_timeout(Duration::from_millis(300)).is_ok();
        client_stop.store(true, Ordering::Release);
        let _ = cancel.shutdown(Shutdown::Both);
        producer.join().unwrap();
        owner.join().unwrap();
        assert!(was_accepted, "test never entered an accepted handler");
        assert!(
            returned_while_client_alive,
            "owner waited for the still-active trickle client"
        );
    }
    #[test]
    fn accepted_trickle_cannot_hold_finish_open() {
        accepted_trickle_shutdown(false);
    }
    #[test]
    fn accepted_trickle_cannot_hold_drop_open() {
        accepted_trickle_shutdown(true);
    }
}
