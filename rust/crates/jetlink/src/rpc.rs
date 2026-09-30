use crate::{
    contract::{self, Identity},
    Deadline, Error,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};
pub const MAX_PACKET: usize = 1 << 20;
pub const REQUEST_BYTES: usize = 29;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub ready: bool,
    pub generation: String,
    pub spec: serde_json::Value,
    pub identity: Identity,
}

pub fn recv_exact(
    stream: &mut UnixStream,
    bytes: &mut [u8],
    deadline: Deadline,
) -> Result<(), Error> {
    let mut read = 0;
    while read < bytes.len() {
        stream.set_read_timeout(Some(deadline.remaining()?))?;
        match stream.read(&mut bytes[read..]) {
            Ok(0) => return Err(Error::Closed),
            Ok(n) => read += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    deadline.remaining()?;
    Ok(())
}
pub fn recv_packet(stream: &mut UnixStream, deadline: Deadline) -> Result<Vec<u8>, Error> {
    let mut size = [0; 4];
    recv_exact(stream, &mut size, deadline)?;
    let size =
        usize::try_from(u32::from_le_bytes(size)).map_err(|_| Error::Contract("packet length"))?;
    if !(1..=MAX_PACKET).contains(&size) {
        return Err(Error::Contract("packet length"));
    }
    let mut bytes = vec![0; size];
    recv_exact(stream, &mut bytes, deadline)?;
    Ok(bytes)
}
pub fn send_packet(
    stream: &mut UnixStream,
    payload: &[u8],
    deadline: Deadline,
) -> Result<(), Error> {
    if !(1..=MAX_PACKET).contains(&payload.len()) {
        return Err(Error::Contract("packet length"));
    }
    let size = u32::try_from(payload.len()).map_err(|_| Error::Contract("packet length"))?;
    for mut bytes in [&size.to_le_bytes()[..], payload] {
        while !bytes.is_empty() {
            stream.set_write_timeout(Some(deadline.remaining()?))?;
            match stream.write(bytes) {
                Ok(0) => return Err(Error::Closed),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    deadline.remaining()?;
    Ok(())
}
pub fn generation(text: &str) -> Result<[u8; 16], Error> {
    if text.len() != 32 || !text.is_ascii() {
        return Err(Error::Contract("generation"));
    }
    let mut out = [0; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
            .map_err(|_| Error::Contract("generation"))?;
    }
    Ok(out)
}
pub fn encode_generation(bytes: [u8; 16]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}

pub struct ProxyClient {
    stream: Option<UnixStream>,
    pub generation: [u8; 16],
    pub identity: Identity,
}
impl ProxyClient {
    pub fn connect(path: &Path) -> Result<Self, Error> {
        let deadline = Deadline::after(Duration::from_millis(500))?;
        let socket = rustix::net::socket_with(
            rustix::net::AddressFamily::UNIX,
            rustix::net::SocketType::STREAM,
            rustix::net::SocketFlags::CLOEXEC | rustix::net::SocketFlags::NONBLOCK,
            None,
        )
        .map_err(std::io::Error::from)?;
        let address = rustix::net::SocketAddrUnix::new(path).map_err(std::io::Error::from)?;
        loop {
            deadline.remaining()?;
            match rustix::net::connect(&socket, &address) {
                Ok(()) => break,
                Err(rustix::io::Errno::AGAIN) => {
                    std::thread::park_timeout(Duration::from_millis(2))
                }
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
        let stream = UnixStream::from(socket);
        stream.set_nonblocking(false)?;
        Self::handshake(stream, deadline)
    }
    pub fn from_stream(stream: UnixStream) -> Result<Self, Error> {
        Self::handshake(stream, Deadline::after(Duration::from_millis(500))?)
    }
    fn handshake(mut stream: UnixStream, deadline: Deadline) -> Result<Self, Error> {
        send_packet(&mut stream, b"H", deadline)?;
        let reply = recv_packet(&mut stream, deadline)?;
        if reply.first() != Some(&b'J') {
            return Err(Error::Contract("owner handshake"));
        }
        let hello: Hello = serde_json::from_slice(&reply[1..])?;
        if !hello.ready || hello.spec != contract::contract()? {
            return Err(Error::Contract("owner readiness/model"));
        }
        Ok(Self {
            stream: Some(stream),
            generation: generation(&hello.generation)?,
            identity: hello.identity,
        })
    }
    pub fn dead(&self) -> bool {
        self.stream.is_none()
    }
    pub fn cancel_handle(&self) -> Result<UnixStream, Error> {
        Ok(self.stream.as_ref().ok_or(Error::Closed)?.try_clone()?)
    }
    pub fn infer(
        &mut self,
        frame: u32,
        warped: &[u8],
        packed: &[f32],
        deadline: Deadline,
        reset: bool,
    ) -> Result<Vec<f32>, Error> {
        let result = self.infer_inner(frame, warped, packed, deadline, reset);
        if result.is_err() {
            self.close();
        }
        result
    }
    fn infer_inner(
        &mut self,
        frame: u32,
        warped: &[u8],
        packed: &[f32],
        deadline: Deadline,
        reset: bool,
    ) -> Result<Vec<f32>, Error> {
        contract::check_input(warped, packed)?;
        let stream = self.stream.as_mut().ok_or(Error::Closed)?;
        let request = encode_request(self.generation, frame, deadline, reset, warped, packed);
        send_packet(stream, &request, deadline)?;
        let reply = recv_packet(stream, deadline)?;
        if reply.len() != 21 + contract::OUTPUT_FLOATS * 4
            || reply[0] != b'R'
            || reply[1..17] != self.generation
            || reply[17..21] != frame.to_le_bytes()
        {
            return Err(Error::Contract("output identity"));
        }
        let output = decode_floats(&reply[21..])?;
        contract::check_output(&output)?;
        deadline.remaining()?;
        Ok(output)
    }
    pub fn close(&mut self) {
        if let Some(stream) = self.stream.take() {
            if let Err(error) = stream.shutdown(Shutdown::Both) {
                if error.kind() != std::io::ErrorKind::NotConnected {
                    eprintln!("jetlink socket close: {error}");
                }
            }
        }
    }
}
impl Drop for ProxyClient {
    fn drop(&mut self) {
        self.close();
    }
}
pub fn encode_request(
    generation: [u8; 16],
    frame: u32,
    deadline: Deadline,
    reset: bool,
    warped: &[u8],
    packed: &[f32],
) -> Vec<u8> {
    let mut request = Vec::with_capacity(1 + REQUEST_BYTES + warped.len() + packed.len() * 4);
    request.push(b'I');
    request.extend(generation);
    request.extend(frame.to_le_bytes());
    request.extend(deadline.0.to_le_bytes());
    request.push(u8::from(reset));
    request.extend(warped);
    for value in packed {
        request.extend(value.to_le_bytes());
    }
    request
}
pub fn decode_floats(bytes: &[u8]) -> Result<Vec<f32>, Error> {
    if !bytes.len().is_multiple_of(4) {
        return Err(Error::Contract("float32 bytes"));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .collect())
}
