//! Standalone recovery framing, sharing the buffered HTTP reader on upgrade.
use crate::{terminal::Terminal, Error, Value};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha1::{Digest, Sha1};
use std::{
    io::{BufReader, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};

struct Writer {
    socket: TcpStream,
    closed: bool,
}

/// Serialized writes shared by the request, PTY and support reader threads.
pub struct Sender(Mutex<Writer>);

impl Sender {
    pub fn is_closed(&self) -> bool {
        self.0.lock().map_or(true, |writer| writer.closed)
    }

    pub fn new(socket: TcpStream) -> Result<Arc<Self>, Error> {
        socket.set_write_timeout(Some(Duration::from_secs(2)))?;
        Ok(Arc::new(Self(Mutex::new(Writer {
            socket,
            closed: false,
        }))))
    }

    pub fn handshake(&self, key: &str) -> Result<(), Error> {
        if !key.is_ascii() {
            return Err(Error::Source("websocket key must be ASCII".into()));
        }
        let accept = STANDARD.encode(Sha1::digest(format!(
            "{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
        )));
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        );
        let mut writer = self
            .0
            .lock()
            .map_err(|_| Error::Source("websocket writer poisoned".into()))?;
        writer.socket.write_all(response.as_bytes())?;
        writer.socket.flush()?;
        Ok(())
    }

    pub fn send_json(&self, value: &Value) -> bool {
        match value.encode() {
            Ok(text) => self.send_frame(text.as_bytes(), 1),
            Err(error) => {
                eprintln!("[recovery] websocket JSON: {error}");
                false
            }
        }
    }

    pub fn send_frame(&self, payload: &[u8], opcode: u8) -> bool {
        let Ok(mut writer) = self.0.lock() else {
            return false;
        };
        if writer.closed {
            return false;
        }
        let mut header = vec![0x80 | opcode];
        if payload.len() < 126 {
            let Ok(length) = u8::try_from(payload.len()) else {
                return false;
            };
            header.push(length);
        } else if let Ok(length) = u16::try_from(payload.len()) {
            header.push(126);
            header.extend_from_slice(&length.to_be_bytes());
        } else if let Ok(length) = u64::try_from(payload.len()) {
            header.push(127);
            header.extend_from_slice(&length.to_be_bytes());
        } else {
            return false;
        }
        let result = writer
            .socket
            .write_all(&header)
            .and_then(|()| writer.socket.write_all(payload))
            .and_then(|()| writer.socket.flush());
        if let Err(error) = result {
            writer.closed = true;
            eprintln!("[recovery] websocket write: {error}");
            if let Err(error) = writer.socket.shutdown(Shutdown::Both) {
                eprintln!("[recovery] websocket failed-write close: {error}");
            }
            return false;
        }
        true
    }

    pub fn close(&self) {
        let _sent = self.send_frame(&[], 8);
        match self.0.lock() {
            Ok(mut writer) => {
                writer.closed = true;
                if let Err(error) = writer.socket.shutdown(Shutdown::Both) {
                    if error.kind() != std::io::ErrorKind::NotConnected {
                        eprintln!("[recovery] websocket close: {error}");
                    }
                }
            }
            Err(error) => eprintln!("[recovery] websocket close lock: {error}"),
        }
    }
}

/// Source accepts both masked/unmasked and binary/text input, replacing bad UTF-8.
pub fn read_message(reader: &mut impl Read, sender: &Sender) -> Result<String, Error> {
    let mut message = Vec::new();
    loop {
        let mut header = [0; 2];
        reader.read_exact(&mut header)?;
        let length = match header[1] & 0x7f {
            126 => {
                let mut bytes = [0; 2];
                reader.read_exact(&mut bytes)?;
                usize::from(u16::from_be_bytes(bytes))
            }
            127 => {
                let mut bytes = [0; 8];
                reader.read_exact(&mut bytes)?;
                usize::try_from(u64::from_be_bytes(bytes))
                    .map_err(|_| Error::Source("websocket frame exceeds address space".into()))?
            }
            length => usize::from(length),
        };
        let mut mask = [0; 4];
        let masked = header[1] & 0x80 != 0;
        if masked {
            reader.read_exact(&mut mask)?;
        }
        let mut raw = Vec::new();
        raw.try_reserve_exact(length)
            .map_err(|error| Error::Source(error.to_string()))?;
        raw.resize(length, 0);
        reader.read_exact(&mut raw)?;
        if masked {
            for (index, byte) in raw.iter_mut().enumerate() {
                *byte ^= mask[index & 3];
            }
        }
        match header[0] & 0x0f {
            8 => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "websocket closed",
                )
                .into())
            }
            9 => {
                let _sent = sender.send_frame(&raw, 10);
                continue;
            }
            10 => continue,
            _ => message.extend_from_slice(&raw),
        }
        if header[0] & 0x80 != 0 {
            return Ok(String::from_utf8_lossy(&message).into_owned());
        }
    }
}

fn text(value: &Value) -> Result<String, Error> {
    if !value.truth() {
        return Ok(String::new());
    }
    match value {
        Value::Text(points) => Ok(points
            .iter()
            .map(|point| char::from_u32(*point).unwrap_or('?'))
            .collect()),
        Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Array(_)
        | Value::Object(_) => Ok(value.string()?),
    }
}

fn dispatch(terminal: &Terminal, data: &Value) -> Result<(), Error> {
    match data.get("type").string()?.as_str() {
        "input" => terminal.write_text(&(text(data.get("data"))? + "\r")),
        "raw" => terminal.write_text(&text(data.get("data"))?),
        "resize" => {
            let value = data.get("rows");
            let rows = if value.truth() {
                value.int()?
            } else {
                30.into()
            };
            use num_traits::ToPrimitive;
            terminal.resize(
                rows.clamp(8.into(), 200.into())
                    .to_i64()
                    .ok_or_else(|| Error::Source("invalid row count".into()))?,
            )
        }
        "control" => {
            let action = data.get("action");
            if action.truth() && !matches!(action, Value::Text(_)) {
                return Err(Error::Source(format!(
                    "'{}' object has no attribute 'strip'",
                    action.type_name()
                )));
            }
            match text(action)?.trim() {
                "ctrl_c" => terminal.write(b"\x03"),
                "clear" => {
                    terminal.clear_history()?;
                    terminal.write(b"clear\r")
                }
                "refresh" => terminal.write(b"\x0c"),
                "detach" => terminal.write(b"\x60d"),
                _ => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

pub fn serve(
    mut reader: BufReader<TcpStream>,
    key: &str,
    terminal: Arc<Terminal>,
    reset: bool,
) -> Result<(), Error> {
    let sender = Sender::new(reader.get_ref().try_clone()?)?;
    sender.handshake(key)?;
    let id = match terminal.attach(Arc::clone(&sender), reset) {
        Ok(id) => id,
        Err(error) => {
            let _sent = sender.send_json(&Value::object([
                ("type", Value::text("error")),
                ("error", Value::text(&error.to_string())),
                ("session", Value::text("recovery-shell")),
            ]));
            sender.close();
            return Ok(());
        }
    };
    while let Ok(message) = read_message(&mut reader, &sender) {
        let Ok(data) = Value::parse(&message) else {
            continue;
        };
        if !matches!(data, Value::Object(_)) {
            break;
        }
        if let Err(error) = dispatch(&terminal, &data) {
            let _sent = sender.send_json(&Value::object([
                ("type", Value::text("error")),
                ("error", Value::text(&error.to_string())),
                ("session", Value::text("recovery-shell")),
            ]));
        }
    }
    let detached = terminal.detach(id);
    sender.close();
    detached
}
