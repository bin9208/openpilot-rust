use super::{Error, Response};
use std::{io::Read, net::TcpStream};

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

pub enum Received {
    Request(Request),
    Response(Response),
}

pub fn receive(stream: &mut TcpStream) -> Result<Received, Error> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let (length, method, path, content_length) = loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > 101 * 65536 {
            return Ok(Received::Response(Response::error(431)));
        }
        let mut headers = [httparse::EMPTY_HEADER; 100];
        let mut parsed = httparse::Request::new(&mut headers);
        let status = match parsed.parse(&bytes) {
            Ok(status) => status,
            Err(httparse::Error::TooManyHeaders) => {
                return Ok(Received::Response(Response::error(431)))
            }
            Err(_) => return Ok(Received::Response(Response::error(400))),
        };
        if let httparse::Status::Complete(length) = status {
            let method = parsed
                .method
                .ok_or(Error::Contract("HTTP method missing"))?
                .to_owned();
            let path = parsed
                .path
                .ok_or(Error::Contract("HTTP path missing"))?
                .to_owned();
            let size = parsed
                .headers
                .iter()
                .find(|header| header.name.eq_ignore_ascii_case("Content-Length"))
                .map_or_else(
                    || super::length::parse(b"0"),
                    |header| super::length::parse(header.value),
                );
            break (length, method, path, size);
        }
    };
    let mut body = Vec::new();
    if method == "POST" {
        let size = match content_length {
            Ok(size) => size,
            Err(message) => return Ok(Received::Response(Response::message(400, &message)?)),
        };
        body.extend_from_slice(&bytes[length..bytes.len().min(length + size)]);
        while body.len() < size {
            let length = buffer.len().min(size - body.len());
            let count = stream.read(&mut buffer[..length])?;
            if count == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..count]);
        }
    }
    Ok(Received::Request(Request { method, path, body }))
}
