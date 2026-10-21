use super::Result;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    time::{Duration, Instant},
};

const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
const ACCEPT: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";
const FRAME_LIMIT: usize = 2 * 1024 * 1024;
const MASK: [u8; 4] = [3, 7, 11, 19];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub opcode: u8,
    pub payload: Vec<u8>,
}

pub struct WebSocketPeer {
    stream: TcpStream,
    buffered: Vec<u8>,
    frame_timeout: Duration,
}

impl WebSocketPeer {
    pub fn set_read_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.stream.set_read_timeout(Some(timeout))?;
        self.frame_timeout = timeout;
        Ok(())
    }
    pub fn connect(port: u16, path: &str, initial_frame: &[u8]) -> Result<Self> {
        if !path.starts_with('/') || path.bytes().any(|byte| byte <= b' ' || byte == 127) {
            return Err("invalid recovery WebSocket path".into());
        }
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        let mut stream = TcpStream::connect_timeout(&address.into(), Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(8)))?;
        stream.set_write_timeout(Some(Duration::from_secs(8)))?;
        let handshake = format!(
            "GET {path} HTTP/1.1\r\nHost: owned\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: {KEY}\r\n\r\n"
        );
        let mut request = handshake.into_bytes();
        request.extend_from_slice(initial_frame);
        stream.write_all(&request)?;
        let mut buffered = Vec::new();
        let mut chunk = [0_u8; 4096];
        let end = loop {
            if let Some(index) = buffered.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
            if buffered.len() >= 65_536 {
                return Err("recovery WebSocket handshake limit exceeded".into());
            }
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                return Err("recovery WebSocket closed during handshake".into());
            }
            buffered.extend_from_slice(&chunk[..count]);
        };
        let header = std::str::from_utf8(&buffered[..end])?;
        if !header.starts_with("HTTP/1.1 101 ")
            || !header.lines().any(|line| {
                line.trim_end_matches('\r')
                    .eq_ignore_ascii_case(&format!("Sec-WebSocket-Accept: {ACCEPT}"))
            })
        {
            return Err("recovery WebSocket upgrade rejected".into());
        }
        let tail = buffered.split_off(end);
        Ok(Self {
            stream,
            buffered: tail,
            frame_timeout: Duration::from_secs(8),
        })
    }

    pub fn send_frame(
        &mut self,
        opcode: u8,
        payload: &[u8],
        fin: bool,
        masked: bool,
    ) -> Result<()> {
        if opcode > 15 || payload.len() > FRAME_LIMIT {
            return Err("invalid recovery WebSocket frame".into());
        }
        let bytes = encode_frame(opcode, payload, fin, masked);
        self.stream.write_all(&bytes)?;
        Ok(())
    }

    pub fn send_json(&mut self, value: &serde_json::Value) -> Result<()> {
        let bytes = serde_json::to_vec(value)?;
        self.send_frame(1, &bytes, true, true)
    }

    pub fn frame(&mut self) -> Result<Frame> {
        let deadline = Instant::now() + self.frame_timeout;
        let mut header = [0_u8; 2];
        self.read_exact_until(&mut header, deadline)?;
        if header[0] & 0x80 == 0 || header[1] & 0x80 != 0 {
            return Err("fragmented or masked server frame".into());
        }
        let mut length = u64::from(header[1] & 127);
        if length == 126 {
            let mut extended = [0_u8; 2];
            self.read_exact_until(&mut extended, deadline)?;
            length = u64::from(u16::from_be_bytes(extended));
        } else if length == 127 {
            let mut extended = [0_u8; 8];
            self.read_exact_until(&mut extended, deadline)?;
            length = u64::from_be_bytes(extended);
        }
        let length: usize = length.try_into()?;
        if length > FRAME_LIMIT {
            return Err("recovery WebSocket frame limit exceeded".into());
        }
        let mut payload = vec![0_u8; length];
        self.read_exact_until(&mut payload, deadline)?;
        Ok(Frame {
            opcode: header[0] & 15,
            payload,
        })
    }

    pub fn event(&mut self, kind: &str) -> Result<serde_json::Value> {
        loop {
            let frame = self.frame()?;
            if frame.opcode == 8 {
                return Err("recovery WebSocket closed before event".into());
            }
            if frame.opcode == 1 {
                let value: serde_json::Value = serde_json::from_slice(&frame.payload)?;
                if value.get("type").and_then(serde_json::Value::as_str) == Some(kind) {
                    return Ok(value);
                }
            }
        }
    }

    pub fn output_until(&mut self, marker: &[u8], limit: usize) -> Result<Vec<u8>> {
        if marker.is_empty() {
            return Err("empty recovery output marker".into());
        }
        let mut output = Vec::new();
        while !output.windows(marker.len()).any(|window| window == marker) {
            let value = self.event("pty_output")?;
            let encoded = value
                .get("b64")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing pty_output b64")?;
            let payload = STANDARD.decode(encoded)?;
            if output.len().saturating_add(payload.len()) > limit {
                return Err("recovery PTY output limit exceeded".into());
            }
            output.extend_from_slice(&payload);
        }
        Ok(output)
    }

    pub fn close(&mut self) -> Result<()> {
        self.send_frame(8, &[], true, true)
    }

    fn read_exact_until(&mut self, out: &mut [u8], deadline: Instant) -> Result<()> {
        let prefix = out.len().min(self.buffered.len());
        out[..prefix].copy_from_slice(&self.buffered[..prefix]);
        self.buffered.drain(..prefix);
        let mut filled = prefix;
        while filled < out.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("recovery WebSocket frame deadline exceeded".into());
            }
            self.stream.set_read_timeout(Some(remaining))?;
            let count = match self.stream.read(&mut out[filled..]) {
                Ok(count) => count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    return Err("recovery WebSocket frame deadline exceeded".into());
                }
                Err(error) => return Err(error.into()),
            };
            if count == 0 {
                return Err("recovery WebSocket closed during frame".into());
            }
            filled += count;
        }
        if Instant::now() >= deadline {
            return Err("recovery WebSocket frame deadline exceeded".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trickled_frame_cannot_extend_total_deadline() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let sender = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&[0x81, 4]).unwrap();
            for byte in b"slow" {
                std::thread::sleep(Duration::from_millis(120));
                if stream.write_all(&[*byte]).is_err() {
                    break;
                }
            }
        });
        let stream = TcpStream::connect(address).unwrap();
        let mut peer = WebSocketPeer {
            stream,
            buffered: Vec::new(),
            frame_timeout: Duration::from_millis(300),
        };
        let error = peer.frame().unwrap_err();
        assert!(
            error.to_string().contains("frame deadline exceeded"),
            "{error}"
        );
        sender.join().unwrap();
    }
}

/// The fixed mask reproduces the unchanged controller's client frames. It is
/// test traffic only; production WebSocket masking is implemented elsewhere.
pub fn encode_frame(opcode: u8, payload: &[u8], fin: bool, masked: bool) -> Vec<u8> {
    let mut bytes = vec![opcode | if fin { 0x80 } else { 0 }];
    let mask_bit = if masked { 0x80 } else { 0 };
    let length = payload.len();
    if length < 126 {
        bytes.push(mask_bit | u8::try_from(length).expect("short frame length"));
    } else if length < 65_536 {
        bytes.push(mask_bit | 126);
        bytes.extend_from_slice(
            &u16::try_from(length)
                .expect("medium frame length")
                .to_be_bytes(),
        );
    } else {
        bytes.push(mask_bit | 127);
        bytes.extend_from_slice(
            &u64::try_from(length)
                .expect("frame length fits u64")
                .to_be_bytes(),
        );
    }
    if masked {
        bytes.extend_from_slice(&MASK);
        bytes.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ MASK[index & 3]),
        );
    } else {
        bytes.extend_from_slice(payload);
    }
    bytes
}
