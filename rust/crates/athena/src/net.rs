use crate::Error;
use base64::{engine::general_purpose::STANDARD, Engine};
use socket2::{SockRef, TcpKeepalive};
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

pub fn tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, Error> {
    let mut last = None;
    for address in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => {
                stream.set_read_timeout(Some(timeout))?;
                stream.set_write_timeout(Some(timeout))?;
                return Ok(stream);
            }
            Err(error) => last = Some(error),
        }
    }
    Err(last
        .unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no resolved address")
        })
        .into())
}

pub fn websocket_tcp(url: &url::Url, timeout: Duration) -> Result<TcpStream, Error> {
    let host = url
        .host_str()
        .ok_or(Error::Contract("WebSocket host missing"))?;
    let port = url
        .port_or_known_default()
        .ok_or(Error::Contract("WebSocket port missing"))?;
    let proxy = proxy(host, url.scheme() == "wss")?;
    let mut stream = if let Some(proxy) = proxy {
        let mut stream = tcp(
            proxy
                .host_str()
                .ok_or(Error::Contract("proxy host missing"))?,
            proxy
                .port_or_known_default()
                .ok_or(Error::Contract("proxy port missing"))?,
            timeout,
        )?;
        let auth = if proxy.username().is_empty() {
            String::new()
        } else {
            format!(
                "Proxy-Authorization: Basic {}\r\n",
                STANDARD.encode(format!(
                    "{}:{}",
                    proxy.username(),
                    proxy.password().unwrap_or("")
                ))
            )
        };
        stream.write_all(
            format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n{auth}\r\n")
                .as_bytes(),
        )?;
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte)?;
            response.push(byte[0]);
        }
        if std::str::from_utf8(&response)?.split_whitespace().nth(1) != Some("200") {
            return Err(Error::Contract("WebSocket proxy CONNECT failed"));
        }
        stream
    } else {
        tcp(host, port, timeout)?
    };
    stream.flush()?;
    stream.set_nodelay(true)?;
    SockRef::from(&stream).set_keepalive(true)?;
    Ok(stream)
}

fn proxy(host: &str, secure: bool) -> Result<Option<url::Url>, Error> {
    let no_proxy = std::env::var("no_proxy")
        .or_else(|_| std::env::var("NO_PROXY"))
        .unwrap_or_else(|_| "localhost,127.0.0.1".into());
    if no_proxy.split(',').any(|entry| {
        let entry = entry.trim();
        entry == "*"
            || host == entry
            || (entry.starts_with('.') && host.ends_with(entry.trim_start_matches('.')))
            || host.ends_with(&format!(".{entry}"))
    }) {
        return Ok(None);
    }
    let key = if secure { "https_proxy" } else { "http_proxy" };
    std::env::var(key)
        .or_else(|_| std::env::var(key.to_ascii_uppercase()))
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| url::Url::parse(&value.replace(' ', "")).map_err(Error::from))
        .transpose()
}

pub fn keepalive(stream: &TcpStream, onroad: bool) -> Result<(), Error> {
    let socket = SockRef::from(stream);
    #[cfg(target_os = "linux")]
    socket.set_tcp_user_timeout(onroad.then_some(Duration::from_millis(16000)))?;
    socket.set_tcp_keepalive(
        &TcpKeepalive::new()
            .with_time(Duration::from_secs(if onroad { 7 } else { 30 }))
            .with_interval(Duration::from_secs(if onroad { 7 } else { 10 }))
            .with_retries(if onroad { 2 } else { 3 }),
    )?;
    Ok(())
}
