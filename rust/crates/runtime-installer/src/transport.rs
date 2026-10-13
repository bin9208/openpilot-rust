use crate::{invalid, Result};
use std::{
    fs::File,
    io::Read,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

pub trait Download {
    fn download(&self, url: &str, destination: &Path, max_bytes: u64) -> Result<()>;
}

#[derive(Debug)]
pub struct CurlDownload;
impl Download for CurlDownload {
    fn download(&self, url: &str, destination: &Path, max_bytes: u64) -> Result<()> {
        if !url.starts_with("https://github.com/bin9208/openpilot-rust/releases/download/runtime-")
        {
            return Err(invalid(
                "download origin is outside runtime release repository",
            ));
        }
        curl_download(url, destination, max_bytes, true)
    }
}
fn curl_download(url: &str, destination: &Path, max_bytes: u64, https_only: bool) -> Result<()> {
    loop {
        let (status, http_status) = curl_attempt(url, destination, max_bytes, https_only)?;
        if status.success() {
            return Ok(());
        }
        match (status.code(), http_status) {
            (Some(code @ (5 | 6 | 7 | 18 | 28 | 35 | 52 | 55 | 56 | 60)), _)
            | (Some(code @ 22), Some(408 | 429 | 500 | 502 | 503 | 504)) => {
                std::fs::remove_file(destination)?;
                println!(
                    "Runtime download waiting for network/time; curl_exit={code} http_status={http_status:?} retry_seconds=5"
                );
                thread::sleep(Duration::from_secs(5));
            }
            _ => return Err(invalid(format!("runtime download failed: {status}"))),
        }
    }
}
fn curl_attempt(
    url: &str,
    destination: &Path,
    max_bytes: u64,
    https_only: bool,
) -> Result<(ExitStatus, Option<u16>)> {
    let protocol = if https_only { "=https" } else { "=http,https" };
    let bound = max_bytes
        .checked_add(1)
        .ok_or_else(|| invalid("download size overflow"))?;
    let mut file = File::create(destination)?;
    let headers = tempfile::NamedTempFile::new()?;
    let mut child = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            protocol,
            "--proto-redir",
            protocol,
            "--connect-timeout",
            "20",
            "--max-time",
            "1800",
            "--max-filesize",
            &max_bytes.to_string(),
            url,
        ])
        .arg("--dump-header")
        .arg(headers.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| invalid("curl stdout unavailable"))?;
    let mut output = output.take(bound);
    let copied = std::io::copy(&mut output, &mut file);
    drop(output);
    match copied {
        Ok(n) if n <= max_bytes => {
            let status = child.wait()?;
            if status.success() {
                file.sync_all()?;
            }
            let mut header_bytes = Vec::new();
            let bytes = File::open(headers.path())?
                .take(65_537)
                .read_to_end(&mut header_bytes)?;
            if bytes > 65_536 {
                return Err(invalid("download headers exceed byte bound"));
            }
            let http_status = header_bytes
                .split(|byte| *byte == b'\n')
                .filter_map(|line| std::str::from_utf8(line).ok())
                .filter(|line| line.starts_with("HTTP/"))
                .filter_map(|line| line.split_ascii_whitespace().nth(1)?.parse::<u16>().ok())
                .next_back();
            Ok((status, http_status))
        }
        result => {
            if let Err(e) = child.kill() {
                eprintln!("installer download cancellation: {e}");
            }
            if let Err(e) = child.wait() {
                eprintln!("installer download reap: {e}");
            }
            match result {
                Ok(_) => Err(invalid("download exceeds declared byte bound")),
                Err(e) => Err(e.into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
    };

    struct Server {
        address: String,
        worker: Option<thread::JoinHandle<()>>,
        requests: Arc<AtomicUsize>,
    }
    impl Server {
        fn new(responses: Vec<Vec<u8>>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let requests = Arc::new(AtomicUsize::new(0));
            let worker_requests = Arc::clone(&requests);
            let worker = thread::spawn(move || {
                for (index, connection) in listener.incoming().enumerate() {
                    let mut stream = connection.unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut bytes = [0; 4096];
                    let n = stream.read(&mut bytes).unwrap();
                    if n == 0 {
                        break;
                    }
                    worker_requests.fetch_add(1, Ordering::SeqCst);
                    let response = &responses[index.min(responses.len() - 1)];
                    stream.write_all(response).unwrap();
                }
            });
            Self {
                address,
                worker: Some(worker),
                requests,
            }
        }
        fn url(&self) -> String {
            format!("http://{}/runtime.tar.gz", self.address)
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            drop(TcpStream::connect(&self.address).unwrap());
            self.worker.take().unwrap().join().unwrap();
        }
    }
    #[test]
    fn interrupted_response_retries_from_zero() {
        // Given an owned HTTP server truncating the first response then serving the complete asset.
        let server = Server::new(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\npar".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncomplete".to_vec(),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset");
        // When the real curl transport handles the download.
        let result = curl_download(&server.url(), &path, 8, false);
        // Then only complete second-attempt bytes remain.
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
    }
    #[test]
    fn delayed_network_recovery_outlasts_three_attempts() {
        let partial =
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\npar".to_vec();
        let server = Server::new(vec![
            partial.clone(),
            partial.clone(),
            partial.clone(),
            partial,
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncomplete".to_vec(),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset");
        let started = std::time::Instant::now();
        let result = curl_download(&server.url(), &path, 8, false);
        assert!(result.is_ok(), "{result:?}");
        assert!(started.elapsed() >= std::time::Duration::from_secs(20));
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
    }
    #[test]
    fn temporary_http_failure_recovers_after_old_retry_budget() {
        let unavailable =
            b"HTTP/1.1 503 Service Unavailable\r\nX-Header: \xff\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec();
        let server = Server::new(vec![
            unavailable.clone(),
            unavailable.clone(),
            unavailable.clone(),
            unavailable,
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncomplete".to_vec(),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset");
        let result = curl_download(&server.url(), &path, 8, false);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
    }
    #[test]
    fn missing_release_is_a_download_failure() {
        // Given a missing release returned by the owned HTTP server.
        let server = Server::new(vec![
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ]);
        let dir = tempfile::tempdir().unwrap();
        // When the transport tries to download it.
        let result = curl_download(&server.url(), &dir.path().join("asset"), 8, false);
        // Then installation receives a failure.
        assert!(result.is_err());
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn oversized_unadvertised_response_is_bounded() {
        // Given a server omitting content length and returning too much data.
        let server = Server::new(vec![
            b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n0123456789012345".to_vec(),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset");
        // When downloading under an eight-byte descriptor bound.
        let result = curl_download(&server.url(), &path, 8, false);
        // Then the overrun is rejected without storing the full body.
        assert!(result.is_err());
        assert!(std::fs::metadata(path).unwrap().len() <= 9);
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);
    }
}
