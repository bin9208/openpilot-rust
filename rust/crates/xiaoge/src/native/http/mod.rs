mod body;
mod length;
mod request;
mod route;
mod target;
use super::{shared::Shared, Error};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    io::Write,
    net::{SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
    thread,
};

pub struct Response {
    status: u16,
    body: Vec<u8>,
    content_type: &'static str,
    cache: bool,
}

impl Response {
    fn json(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
            cache: false,
        }
    }
    fn message(status: u16, message: &str) -> Result<Self, Error> {
        use openpilot_logging::Value;
        Ok(Self::json(
            status,
            crate::wire::fields([("error", Value::Text(message.to_owned()))])
                .to_json()?
                .into_bytes(),
        ))
    }
    fn error(status: u16) -> Self {
        let (reason, explanation) = match status {
            400 => ("Bad Request", "Bad request syntax or unsupported method"),
            404 => ("Not Found", "Nothing matches the given URI"),
            431 => ("Request Header Fields Too Large", "Too many headers"),
            _ => ("Not Implemented", "Server does not support this operation"),
        };
        let body = format!("<!DOCTYPE HTML>\n<html lang=\"en\">\n    <head>\n        <meta charset=\"utf-8\">\n        <title>Error response</title>\n    </head>\n    <body>\n        <h1>Error response</h1>\n        <p>Error code: {status}</p>\n        <p>Message: {reason}.</p>\n        <p>Error code explanation: {status} - {explanation}.</p>\n    </body>\n</html>\n");
        Self {
            status,
            body: body.into_bytes(),
            content_type: "text/html;charset=utf-8",
            cache: true,
        }
    }

    fn send(self, stream: &mut TcpStream) -> Result<(), Error> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            431 => "Request Header Fields Too Large",
            501 => "Not Implemented",
            503 => "Service Unavailable",
            _ => "Error",
        };
        write!(stream, "HTTP/1.0 {} {}\r\nServer: openpilot-rust\r\nDate: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n",
            self.status, reason, httpdate::fmt_http_date(std::time::SystemTime::now()), self.content_type, self.body.len())?;
        if !self.cache {
            stream.write_all(b"Cache-Control: no-store\r\n")?;
        }
        stream.write_all(b"\r\n")?;
        stream.write_all(&self.body)?;
        Ok(())
    }
}

pub fn bind(address: SocketAddrV4) -> Result<TcpListener, Error> {
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::V4(address).into())?;
    socket.listen(5)?;
    Ok(socket.into())
}

pub fn run(listener: TcpListener, shared: Arc<Shared>, index: PathBuf) -> Result<(), Error> {
    println!(
        "Xiaoge vision server listening on port {}",
        listener.local_addr()?.port()
    );
    while shared.running.load(Ordering::Acquire) {
        let (mut stream, _) = listener.accept()?;
        let shared = Arc::clone(&shared);
        let index = index.clone();
        thread::Builder::new()
            .name("xiaoge-http-client".to_owned())
            .spawn(move || {
                if let Err(error) = request::receive(&mut stream)
                    .and_then(|request| route::respond(request, &shared, &index))
                    .and_then(|response| response.send(&mut stream))
                {
                    match &error {
                        Error::Io(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::BrokenPipe
                                    | std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::ConnectionAborted
                            ) => {}
                        _ => eprintln!("Xiaoge HTTP request failed: {error}"),
                    }
                }
            })?;
    }
    Ok(())
}
