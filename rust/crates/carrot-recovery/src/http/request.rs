use crate::Error;
use num_traits::ToPrimitive;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
};

pub enum ReadError {
    Io(std::io::Error),
    Framework(u16, String),
}
impl From<std::io::Error> for ReadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
fn line(reader: &mut BufReader<TcpStream>) -> Result<Vec<u8>, ReadError> {
    let mut bytes = Vec::new();
    reader.by_ref().take(65537).read_until(b'\n', &mut bytes)?;
    Ok(bytes)
}
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}
fn utf8(bytes: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(bytes).map_err(|failure| {
        let start = failure.valid_up_to();
        let length = failure.error_len().unwrap_or(bytes.len() - start);
        let reason = match failure.error_len() {
            None => "unexpected end of data",
            Some(_) if (0xc2..=0xf4).contains(&bytes[start]) => "invalid continuation byte",
            Some(_) => "invalid start byte",
        };
        let subject = if length == 1 {
            format!("byte 0x{:02x} in position {start}", bytes[start])
        } else {
            format!("bytes in position {start}-{}", start + length - 1)
        };
        Error::Source(format!("'utf-8' codec can't decode {subject}: {reason}"))
    })
}
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: Vec<(String, String)>,
    pub persistent: bool,
}
impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
    pub fn read(reader: &mut BufReader<TcpStream>) -> Result<Option<Self>, ReadError> {
        let bytes = line(reader)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.len() > 65536 {
            return Err(ReadError::Framework(414, String::new()));
        }
        let request_line = latin1(&bytes);
        let parts: Vec<_> = request_line.split_whitespace().collect();
        let [method, target, version] = parts.as_slice() else {
            return Err(ReadError::Framework(400, "Bad request syntax".into()));
        };
        let method = (*method).to_owned();
        let target = (*target).to_owned();
        let persistent = *version == "HTTP/1.1";
        let mut headers = Vec::new();
        let mut header_lines = 0;
        loop {
            let bytes = line(reader)?;
            if bytes.len() > 65536 {
                return Err(ReadError::Framework(431, "Line too long".into()));
            }
            header_lines += 1;
            if header_lines > 100 {
                return Err(ReadError::Framework(431, "Too many headers".into()));
            }
            if bytes == b"\r\n" || bytes == b"\n" || bytes.is_empty() {
                break;
            }
            let value = latin1(&bytes);
            if let Some((name, value)) = value.split_once(':') {
                headers.push((name.to_owned(), value.trim().to_owned()));
            }
        }
        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        let query = query
            .split('&')
            .filter(|piece| !piece.is_empty())
            .map(|piece| {
                let (key, value) = piece.split_once('=').unwrap_or((piece, ""));
                (key.to_owned(), value.to_owned())
            })
            .collect();
        let mut request = Self {
            method,
            path: path.into(),
            query,
            headers,
            persistent,
        };
        if request
            .header("Connection")
            .is_some_and(|value| value.eq_ignore_ascii_case("close"))
        {
            request.persistent = false;
        }
        if persistent
            && request
                .header("Expect")
                .is_some_and(|value| value.eq_ignore_ascii_case("100-continue"))
        {
            reader
                .get_mut()
                .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
            reader.get_mut().flush()?;
        }
        Ok(Some(request))
    }
    pub fn json(&self, reader: &mut BufReader<TcpStream>) -> Result<crate::Value, Error> {
        let encoded_length = self
            .header("Content-Length")
            .filter(|value| !value.is_empty())
            .unwrap_or("0");
        let length = crate::Value::text(encoded_length).int()?;
        if length > 65536.into() {
            return Err(Error::Source("request too large".into()));
        }
        let length = length
            .to_usize()
            .ok_or_else(|| Error::Source("invalid Content-Length".into()))?;
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        let body = utf8(&body)?;
        Ok(crate::Value::parse(if body.is_empty() {
            "{}"
        } else {
            body
        })?)
    }
}
