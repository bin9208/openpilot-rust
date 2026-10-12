use crate::{Error, Value};
use std::{io::Write, net::TcpStream, time::SystemTime};

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub cache: Option<bool>,
    pub headers: Vec<(String, String)>,
}
impl Response {
    pub fn bytes(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
            cache: Some(false),
            headers: Vec::new(),
        }
    }
    pub fn json(status: u16, value: Value) -> Result<Self, Error> {
        let encoded = value.encode()?;
        let body = openpilot_logmessaged::JsonValue::parse(&encoded)
            .map_err(|error| Error::Source(error.to_string()))?
            .to_json_utf8()
            .map_err(|_| Error::Source("cannot encode lone surrogate as UTF-8".into()))?;
        Ok(Self::bytes(
            status,
            "application/json; charset=utf-8",
            body.into_bytes(),
        ))
    }
    pub fn send(&self, socket: &mut TcpStream, head: bool) -> Result<(), Error> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            409 => "Conflict",
            503 => "Service Unavailable",
            501 => "Not Implemented",
            414 => "Request-URI Too Long",
            431 => "Request Header Fields Too Large",
            _ => "Error",
        };
        write!(socket, "HTTP/1.1 {} {}\r\nServer: CarrotRecovery/2.0 Rust\r\nDate: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n", self.status, reason, httpdate::fmt_http_date(SystemTime::now()), self.content_type, self.body.len())?;
        if let Some(cache) = self.cache {
            write!(
                socket,
                "Cache-Control: {}\r\n",
                if cache {
                    "public, max-age=300"
                } else {
                    "no-store"
                }
            )?;
        }
        for (name, value) in &self.headers {
            write!(socket, "{name}: {value}\r\n")?;
        }
        socket.write_all(b"\r\n")?;
        if !head {
            socket.write_all(&self.body)?;
        }
        socket.flush()?;
        Ok(())
    }
    pub fn framework(status: u16, message: &str) -> Self {
        let (phrase, explanation) = match status {
            400 => ("Bad Request", "Bad request syntax or unsupported method"),
            404 => ("Not Found", "Nothing matches the given URI"),
            414 => ("Request-URI Too Long", "URI is too long"),
            431 if message == "Line too long" => (
                "Request Header Fields Too Large",
                "got more than 65536 bytes when reading header line",
            ),
            431 => (
                "Request Header Fields Too Large",
                "got more than 100 headers",
            ),
            501 => ("Not Implemented", "Server does not support this operation"),
            _ => ("Internal Server Error", "Server got itself in trouble"),
        };
        let message = if message.is_empty() { phrase } else { message };
        let message = message
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let body = format!("<!DOCTYPE HTML>\n<html lang=\"en\">\n    <head>\n        <meta charset=\"utf-8\">\n        <title>Error response</title>\n    </head>\n    <body>\n        <h1>Error response</h1>\n        <p>Error code: {status}</p>\n        <p>Message: {message}.</p>\n        <p>Error code explanation: {status} - {explanation}.</p>\n    </body>\n</html>\n");
        let mut response = Self::bytes(status, "text/html;charset=utf-8", body.into_bytes());
        response.cache = None;
        response.headers.push(("Connection".into(), "close".into()));
        response
    }
}
