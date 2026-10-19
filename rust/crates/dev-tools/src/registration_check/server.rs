//! Private loopback HTTP fixture, matching the source checker's real exchanges.
use super::values as v;
use crate::seal::Result;
use openpilot_log_format::JsonValue;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

struct State {
    rows: Vec<JsonValue>,
    responses: VecDeque<JsonValue>,
}
pub(super) struct Server {
    pub(super) url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub(super) fn new(case: &JsonValue) -> Result<Self> {
        let responses = case.get("responses").map_or_else(
            || Ok(vec![v::value(serde_json::json!({}))?]),
            |v| v::items(&v),
        )?;
        let state = Arc::new(Mutex::new(State {
            rows: Vec::new(),
            responses: responses.into(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let url = format!("http://{}", listener.local_addr()?);
        listener.set_nonblocking(true)?;
        let shared = Arc::clone(&state);
        let halted = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut clients = Vec::new();
            while !halted.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&shared);
                        clients.push(thread::spawn(move || {
                            if let Err(error) = respond(stream, &state) {
                                eprintln!("registration fixture connection: {error}")
                            }
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => {
                        eprintln!("registration HTTP accept: {error}");
                        break;
                    }
                }
            }
            for client in clients {
                let _ = client.join();
            }
        });
        Ok(Self {
            url,
            state,
            stop,
            thread: Some(thread),
        })
    }
    pub(super) fn reset(&self, case: &JsonValue) -> Result<()> {
        let responses = case.get("responses").map_or_else(
            || Ok(vec![v::value(serde_json::json!({}))?]),
            |value| v::items(&value),
        )?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "HTTP fixture lock poisoned")?;
        state.rows.clear();
        state.responses = responses.into();
        Ok(())
    }
    pub(super) fn rows(&self) -> Result<JsonValue> {
        v::array(
            self.state
                .lock()
                .map_err(|_| "HTTP fixture lock poisoned")?
                .rows
                .clone(),
        )
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn line(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let mut one = [0];
        if stream.read(&mut one)? == 0 {
            return Err("HTTP request ended before a line".into());
        }
        bytes.push(one[0]);
        if bytes.ends_with(b"\r\n") {
            bytes.truncate(bytes.len() - 2);
            return Ok(bytes);
        }
        if bytes.len() > 1024 * 1024 {
            return Err("HTTP fixture line exceeds bounded source fixture input".into());
        }
    }
}
fn respond(mut stream: TcpStream, state: &Mutex<State>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    let request = String::from_utf8(line(&mut stream)?)?;
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        let raw = line(&mut stream)?;
        if raw.is_empty() {
            break;
        }
        let text = String::from_utf8(raw)?;
        let (key, value) = text
            .split_once(':')
            .ok_or("malformed fixture request header")?;
        let value = value.trim_start_matches([' ', '\t']);
        if key.eq_ignore_ascii_case("content-length") {
            length = value.parse::<usize>()?
        }
        headers.push(v::value([key, value])?)
    }
    if length > 16 * 1024 * 1024 {
        return Err("fixture request body too large".into());
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    let response = {
        let mut state = state.lock().map_err(|_| "HTTP fixture lock poisoned")?;
        state.rows.push(v::object([
            ("line".into(), v::text(&request)),
            ("headers".into(), v::array(headers)?),
            ("body_hex".into(), v::text(&v::hex(&body))),
        ])?);
        state
            .responses
            .pop_front()
            .unwrap_or(v::value(serde_json::json!({"status":403}))?)
    };
    let status = response.get("status").map_or(Ok(200), |n| v::integer(&n))?;
    let mut body = if let Some(hex) = response.get("body_hex") {
        v::unhex(&v::string(&hex)?)?
    } else {
        let text = response
            .get("body")
            .map_or(Ok("{\"dongle_id\":\"registered-synthetic\"}".into()), |v| {
                v::string(&v)
            })?;
        let encoding = response
            .get("encoding")
            .map_or(Ok("utf-8".into()), |v| v::string(&v))?;
        super::http::encode(&text, &encoding)?
    };
    if v::flag(&response, "gzip") {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&body)?;
        body = encoder.finish()?;
    } else if v::flag(&response, "deflate") {
        if response.get("deflate").is_some_and(|v| v.text_eq("raw")) {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&body)?;
            body = encoder.finish()?;
        } else {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&body)?;
            body = encoder.finish()?;
        }
    } else if v::flag(&response, "brotli") {
        let mut compressed = Vec::new();
        {
            let mut encoder = brotli::CompressorWriter::new(&mut compressed, 4096, 11, 22);
            encoder.write_all(&body)?;
        }
        body = compressed;
    }
    if v::flag(&response, "disconnect") {
        return Ok(());
    }
    delay(&response, "header_delay")?;
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    if let Some(content) = response.get("content_type") {
        if !v::is_null(&content) {
            head.push_str(&format!("Content-Type: {}\r\n", v::string(&content)?))
        }
    } else {
        head.push_str("Content-Type: application/json\r\n")
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n",
        body.len()
    ));
    if v::flag(&response, "gzip") {
        head.push_str("Content-Encoding: gzip\r\n")
    } else if v::flag(&response, "deflate") {
        head.push_str("Content-Encoding: deflate\r\n")
    } else if v::flag(&response, "brotli") {
        head.push_str("Content-Encoding: br\r\n")
    }
    if let Some(location) = response.get("location") {
        head.push_str(&format!("Location: {}\r\n", v::string(&location)?));
    }
    if let Some(headers) = response.get("headers") {
        if let openpilot_log_format::JsonView::Object(values) = headers.view() {
            for (key, value) in values {
                head.push_str(&format!(
                    "{}: {}\r\n",
                    v::scalar_utf8(key)?,
                    v::string(&value)?
                ))
            }
        } else {
            for pair in v::items(&headers)? {
                let pair = v::items(&pair)?;
                if pair.len() != 2 {
                    return Err("extra HTTP header is not a pair".into());
                }
                head.push_str(&format!(
                    "{}: {}\r\n",
                    v::string(&pair[0])?,
                    v::string(&pair[1])?
                ))
            }
        }
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    delay(&response, "body_delay")?;
    let write = if let Some(drip) = response.get("drip") {
        let interval = Duration::from_secs_f64(v::number(&drip)?);
        stream.write_all(body.get(..1).unwrap_or(&body))?;
        stream.flush()?;
        thread::sleep(interval);
        stream.write_all(body.get(1..2).unwrap_or(&[]))?;
        stream.flush()?;
        thread::sleep(interval);
        stream.write_all(body.get(2..).unwrap_or(&[]))
    } else {
        stream.write_all(&body)
    };
    match write {
        Ok(()) => Ok(()),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ) =>
        {
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
fn delay(response: &JsonValue, name: &str) -> Result<()> {
    if let Some(seconds) = response.get(name) {
        thread::sleep(Duration::from_secs_f64(v::number(&seconds)?));
    }
    Ok(())
}
fn reason(status: i64) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}
