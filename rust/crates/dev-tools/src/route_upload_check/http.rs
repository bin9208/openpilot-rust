//! Synthetic loopback recipient and signing material shared by both controllers.
use crate::seal::Result;
use p256::pkcs8::{EncodePublicKey, LineEnding};
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}
pub(super) fn signing_key(persist: &Path) -> Result<()> {
    let private = p256::SecretKey::random(&mut rsa::rand_core::OsRng);
    let directory = persist.join("comma");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("id_ecdsa"),
        private.to_sec1_pem(LineEnding::LF)?.as_bytes(),
    )?;
    fs::write(
        directory.join("id_ecdsa.pub"),
        private.public_key().to_public_key_pem(LineEnding::LF)?,
    )?;
    Ok(())
}
const HEADER_LIMIT: usize = 64 * 1024;
const BODY_LIMIT: usize = 64 * 1024 * 1024;
fn receive(mut stream: TcpStream, url: &str, rows: &Mutex<Vec<Request>>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        super::require(
            head.len() < HEADER_LIMIT,
            "upload fixture header limit exceeded",
        )?;
        let mut one = [0];
        stream.read_exact(&mut one)?;
        head.push(one[0]);
    }
    let text = std::str::from_utf8(&head)?;
    let mut lines = text.split("\r\n");
    let request = lines
        .next()
        .ok_or("HTTP request missing")?
        .split_whitespace()
        .collect::<Vec<_>>();
    super::require(
        request.len() == 3 && request[2].starts_with("HTTP/1."),
        "invalid fixture HTTP request",
    )?;
    let mut length = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("invalid fixture HTTP header")?;
        if name.eq_ignore_ascii_case("content-length") {
            super::require(length.is_none(), "duplicate fixture content-length")?;
            length = Some(value.trim().parse::<usize>()?);
        }
        super::require(
            !name.eq_ignore_ascii_case("transfer-encoding"),
            "unexpected fixture transfer encoding",
        )?;
    }
    let length = length.unwrap_or(0);
    super::require(length <= BODY_LIMIT, "upload fixture body limit exceeded")?;
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    let (method, path) = (request[0].to_owned(), request[1].to_owned());
    super::require(
        method == "GET" || method == "PUT",
        "unexpected upload fixture method",
    )?;
    let response = if method == "GET" && path.starts_with("/v1.4/") {
        serde_json::to_vec(
            &json!({"url":format!("{url}/put"),"headers":{"X-Fixture":"synthetic"}}),
        )?
    } else if method == "PUT" {
        b"uploaded".to_vec()
    } else {
        b"done".to_vec()
    };
    rows.lock()
        .map_err(|_| "upload fixture lock poisoned")?
        .push(Request { method, path, body });
    write!(
        stream,
        "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.len()
    )?;
    stream.write_all(&response)?;
    Ok(())
}
pub(super) struct Server {
    pub(super) url: String,
    rows: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<std::result::Result<(), String>>>,
}
impl Server {
    pub(super) fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let rows = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (captured, halted, host) = (rows.clone(), stop.clone(), url.clone());
        let worker = thread::spawn(move || {
            let run = || -> Result<()> {
                while !halted.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => receive(stream, &host, &captured)?,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                Ok(())
            };
            run().map_err(|error| error.to_string())
        });
        Ok(Self {
            url,
            rows,
            stop,
            worker: Some(worker),
        })
    }
    pub(super) fn finish(mut self) -> Result<Vec<Request>> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "upload recipient panicked")?
                .map_err(std::io::Error::other)?;
        }
        let rows = self
            .rows
            .lock()
            .map_err(|_| "upload fixture lock poisoned")?
            .clone();
        Ok(rows)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_captures_binary_body_and_rejects_ambiguous_framing() {
        let server = Server::new().unwrap();
        let mut stream = TcpStream::connect(server.url.strip_prefix("http://").unwrap()).unwrap();
        stream
            .write_all(b"PUT /put HTTP/1.1\r\nContent-Length: 3\r\n\r\n\x00\xffx")
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        assert!(response.starts_with(b"HTTP/1.0 200"));
        let rows = server.finish().unwrap();
        assert_eq!(rows[0].body, b"\x00\xffx");
        let server = Server::new().unwrap();
        let mut stream = TcpStream::connect(server.url.strip_prefix("http://").unwrap()).unwrap();
        stream
            .write_all(b"PUT /put HTTP/1.1\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\nabc")
            .unwrap();
        let _ = stream.read_to_end(&mut Vec::new());
        assert!(server.finish().is_err());
    }
}
