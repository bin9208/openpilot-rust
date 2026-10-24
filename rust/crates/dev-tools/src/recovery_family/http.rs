use super::Result;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    time::{Duration, Instant},
};

const HEADER_LIMIT: usize = 65_536;
const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// One loopback connection. A fresh connection for each request matches the
/// source controller's inactive HTTP routes and avoids response pipelining.
pub struct HttpPeer {
    port: u16,
    timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub interim: Vec<u16>,
}

impl HttpPeer {
    pub const fn new(port: u16, timeout: Duration) -> Self {
        Self { port, timeout }
    }

    pub fn exchange(&self, wire: &[u8]) -> Result<HttpResponse> {
        self.exchange_with_body_policy(wire, false)
    }

    fn exchange_with_body_policy(&self, wire: &[u8], suppress_body: bool) -> Result<HttpResponse> {
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.port);
        let mut stream = TcpStream::connect_timeout(&address.into(), self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        stream.write_all(wire)?;
        read_response(&mut stream, suppress_body)
    }

    pub fn exchange_continue(&self, head: &[u8], body: &[u8]) -> Result<HttpResponse> {
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.port);
        let mut stream = TcpStream::connect_timeout(&address.into(), self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        stream.write_all(head)?;
        let (interim_head, _) = read_head(&mut stream)?;
        let (status, _) = parse_head(&interim_head)?;
        if status != 100 {
            return Err("recovery server omitted 100 Continue".into());
        }
        stream.write_all(body)?;
        let mut response = read_response(&mut stream, false)?;
        response.interim.insert(0, 100);
        Ok(response)
    }

    pub fn request(&self, method: &str, path: &str, body: &[u8]) -> Result<HttpResponse> {
        if !method.bytes().all(|byte| byte.is_ascii_uppercase())
            || !path.starts_with('/')
            || path.bytes().any(|byte| byte <= b' ' || byte == 127)
        {
            return Err("invalid local HTTP request".into());
        }
        let entity_headers = if method == "POST" {
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            )
        } else {
            String::new()
        };
        let header = format!(
            "{method} {path} HTTP/1.1\r\nHost: owned\r\nConnection: close\r\n{entity_headers}\r\n"
        );
        let mut wire = Vec::with_capacity(header.len() + body.len());
        wire.extend_from_slice(header.as_bytes());
        wire.extend_from_slice(body);
        self.exchange_with_body_policy(&wire, method == "HEAD")
    }

    pub fn wait_ready(&self, path: &str, deadline: Duration) -> Result<()> {
        let end = Instant::now() + deadline;
        loop {
            if matches!(self.request("GET", path, &[]), Ok(response) if response.status == 200) {
                return Ok(());
            }
            if Instant::now() >= end {
                return Err("recovery HTTP readiness timeout".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

pub(super) fn read_head(stream: &mut TcpStream) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        if let Some(end) = header_end(&buffer) {
            return Ok((buffer[..end].to_vec(), buffer[end..].to_vec()));
        }
        if buffer.len() >= HEADER_LIMIT {
            return Err("recovery HTTP header limit exceeded".into());
        }
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err("recovery HTTP closed before headers".into());
        }
        buffer.extend_from_slice(&chunk[..count]);
    }
}

fn parse_head(head: &[u8]) -> Result<(u16, Vec<(String, String)>)> {
    let text = std::str::from_utf8(head)?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next().ok_or("missing HTTP status line")?;
    let mut parts = status_line.split_ascii_whitespace();
    let version = parts.next().ok_or("missing HTTP version")?;
    if !version.starts_with("HTTP/") {
        return Err("invalid HTTP response version".into());
    }
    let status: u16 = parts.next().ok_or("missing HTTP status")?.parse()?;
    let mut headers = Vec::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or("malformed HTTP response header")?;
        headers.push((name.to_ascii_lowercase(), value.trim_ascii().to_owned()));
    }
    Ok((status, headers))
}

fn read_response(stream: &mut TcpStream, suppress_body: bool) -> Result<HttpResponse> {
    let mut interim = Vec::new();
    let (mut head, mut body) = read_head(stream)?;
    let (status, headers) = loop {
        let (status, headers) = parse_head(&head)?;
        if status != 100 {
            break (status, headers);
        }
        interim.push(status);
        // A read can contain both the interim and final responses.
        if let Some(end) = header_end(&body) {
            head = body[..end].to_vec();
            body = body[end..].to_vec();
        } else {
            let (next_head, next_body) = read_head_with_prefix(stream, body)?;
            head = next_head;
            body = next_body;
        }
    };
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .map(|(_, value)| value.parse::<usize>())
        .transpose()?;
    if length.is_some_and(|size| size > BODY_LIMIT) {
        return Err("recovery HTTP body limit exceeded".into());
    }
    if suppress_body || status == 204 || status == 304 {
        body.clear();
    } else if let Some(length) = length {
        while body.len() < length {
            if !read_more(stream, &mut body)? {
                return Err("recovery HTTP body ended before Content-Length".into());
            }
        }
        body.truncate(length);
    } else if status != 101 {
        while read_more(stream, &mut body)? && body.len() <= BODY_LIMIT {}
    }
    if body.len() > BODY_LIMIT {
        return Err("recovery HTTP body limit exceeded".into());
    }
    Ok(HttpResponse {
        status,
        headers,
        body,
        interim,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_socket_cannot_satisfy_declared_body_length() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let sender = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 512];
            stream.read(&mut request).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nab")
                .unwrap();
        });
        let peer = HttpPeer::new(port, Duration::from_secs(2));
        let error = peer.request("GET", "/", &[]).unwrap_err();
        sender.join().unwrap();
        assert!(
            error.to_string().contains("before Content-Length"),
            "{error}"
        );
    }
}

fn read_head_with_prefix(
    stream: &mut TcpStream,
    mut prefix: Vec<u8>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    while header_end(&prefix).is_none() {
        if prefix.len() >= HEADER_LIMIT || !read_more(stream, &mut prefix)? {
            return Err("recovery HTTP closed before final headers".into());
        }
    }
    let end = header_end(&prefix).ok_or("missing final header terminator")?;
    Ok((prefix[..end].to_vec(), prefix[end..].to_vec()))
}

fn read_more(stream: &mut TcpStream, body: &mut Vec<u8>) -> Result<bool> {
    let mut chunk = [0_u8; 4096];
    let count = stream.read(&mut chunk)?;
    body.extend_from_slice(&chunk[..count]);
    Ok(count != 0)
}

/// Date and Server are explicitly dynamic in the unchanged source controller.
pub fn stable_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(name, _)| name != "date" && name != "server")
        .cloned()
        .collect()
}
