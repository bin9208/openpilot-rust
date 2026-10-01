use crate::Error;
use base64::{engine::general_purpose::STANDARD, Engine};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use socket2::{Domain, SockRef, Socket, TcpKeepalive, Type};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    time::{Duration, Instant},
};

pub(crate) fn connect(address: &SocketAddr, timeout: Duration) -> std::io::Result<TcpStream> {
    if timeout.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "zero connect timeout",
        ));
    }
    let socket = Socket::new(Domain::for_address(*address), Type::STREAM, None)?;
    socket.set_nonblocking(true)?;
    let started = Instant::now();
    match socket.connect(&(*address).into()) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) || rustix::io::Errno::from_io_error(&error)
                == Some(rustix::io::Errno::INPROGRESS) =>
        {
            loop {
                let remaining = timeout
                    .checked_sub(started.elapsed())
                    .filter(|value| !value.is_zero())
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::TimedOut, "connection timed out")
                    })?;
                let duration = Timespec::try_from(remaining).map_err(std::io::Error::other)?;
                let mut fds = [PollFd::new(&socket, PollFlags::OUT)];
                match poll(&mut fds, Some(&duration)) {
                    Ok(0) | Err(rustix::io::Errno::INTR) => continue,
                    Ok(_) => {
                        if let Some(error) = socket.take_error()? {
                            return Err(error);
                        }
                        if fds[0]
                            .revents()
                            .intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
                        {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::ConnectionAborted,
                                "connection closed while connecting",
                            ));
                        }
                        break;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Err(error) => return Err(error),
    }
    socket.set_nonblocking(false)?;
    Ok(socket.into())
}

pub fn tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, Error> {
    let mut last = None;
    for address in (host, port).to_socket_addrs()? {
        match connect(&address, timeout) {
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
