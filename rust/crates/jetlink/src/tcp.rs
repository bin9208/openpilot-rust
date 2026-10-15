//! Deadline-bounded TCP framing; cable connections use the same stream ABI.
//! Source: third_party/jetlink/jetlink/transport/tcp.py (MIT, Zeph Leggett).
use crate::{
    client::{Message, Transport},
    wire::{self, Header, Version},
    Deadline, Error,
};
use std::{
    io::{IoSlice, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    ops::Range,
};
pub struct TcpTransport {
    stream: TcpStream,
    version: Version,
    received: Vec<u8>,
    start: usize,
    end: usize,
    desynced: bool,
}
/// Independent write half: progress/control and creator-thread replies share
/// one serialized writer without locking or moving the receive allocation.
pub struct TcpWriter {
    stream: TcpStream,
    version: Version,
}
impl TcpWriter {
    pub fn send_parts(
        &mut self,
        kind: u16,
        sequence: u32,
        payloads: &[&[u8]],
        flags: u32,
        deadline: Deadline,
    ) -> Result<(), Error> {
        send_segments(
            &mut self.stream,
            self.version,
            kind,
            sequence,
            payloads,
            flags,
            deadline,
        )
    }
}
impl TcpTransport {
    pub fn connect(
        address: SocketAddr,
        version: Version,
        deadline: Deadline,
    ) -> Result<Self, Error> {
        Self::from_stream(
            TcpStream::connect_timeout(&address, deadline.remaining()?)?,
            version,
        )
    }
    pub fn from_stream(stream: TcpStream, version: Version) -> Result<Self, Error> {
        if let Err(error) = stream.set_nodelay(true) {
            eprintln!("Jetlink TCP NODELAY: {error}");
        }
        for result in [
            rustix::net::sockopt::set_socket_send_buffer_size(&stream, 4 << 20),
            rustix::net::sockopt::set_socket_recv_buffer_size(&stream, 4 << 20),
            rustix::net::sockopt::set_socket_keepalive(&stream, true),
        ] {
            if let Err(error) = result {
                eprintln!("Jetlink TCP tuning: {error}");
            }
        }
        Ok(Self {
            stream,
            version,
            received: vec![0; 1 << 20],
            start: 0,
            end: 0,
            desynced: false,
        })
    }
    pub fn writer(&self) -> Result<TcpWriter, Error> {
        Ok(TcpWriter {
            stream: self.stream.try_clone()?,
            version: self.version,
        })
    }
    pub fn cancel_handle(&self) -> Result<TcpStream, Error> {
        Ok(self.stream.try_clone()?)
    }
    pub fn desynced(&self) -> bool {
        self.desynced
    }
    /// Discard a malformed stream until peer closure or a full quiet period.
    /// A server stop interrupts the drain without reopening the same stream.
    pub fn drain(
        &mut self,
        quiet: std::time::Duration,
        stop: &std::sync::atomic::AtomicBool,
    ) -> Result<(), Error> {
        use std::{
            sync::atomic::Ordering,
            time::{Duration, Instant},
        };
        let mut bytes = vec![0; 256 * 1024];
        let mut last = Instant::now();
        while !stop.load(Ordering::Acquire) && last.elapsed() < quiet {
            let remaining = quiet.saturating_sub(last.elapsed());
            if remaining.is_zero() {
                break;
            }
            self.stream
                .set_read_timeout(Some(remaining.min(Duration::from_millis(100))))?;
            match self.stream.read(&mut bytes) {
                Ok(0) => return Ok(()),
                Ok(_) => last = Instant::now(),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::Interrupted
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn fill(&mut self, count: usize, deadline: Deadline) -> Result<(), Error> {
        if self.start + count > self.received.len() {
            self.received.copy_within(self.start..self.end, 0);
            self.end -= self.start;
            self.start = 0;
        }
        if count > self.received.len() {
            self.received.resize(count.max(self.received.len() * 2), 0);
        }
        while self.end - self.start < count {
            self.stream.set_read_timeout(Some(deadline.remaining()?))?;
            let missing = count - (self.end - self.start);
            match self
                .stream
                .read(&mut self.received[self.end..self.end + missing])
            {
                Ok(0) => return Err(Error::Closed),
                Ok(size) => self.end += size,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    return Err(Error::Deadline)
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    /// Send header and borrowed staging memory in one scatter/gather write.
    /// The caller retains the payload until this bounded synchronous send returns.
    pub fn send_view(
        &mut self,
        kind: u16,
        sequence: u32,
        payload: &[u8],
        flags: u32,
        deadline: Deadline,
    ) -> Result<(), Error> {
        self.send_parts(kind, sequence, &[payload], flags, deadline)
    }
    /// Up to six borrowed payload segments (response header, output, optional
    /// telemetry); all descriptor storage stays on the owner thread stack.
    pub fn send_parts(
        &mut self,
        kind: u16,
        sequence: u32,
        payloads: &[&[u8]],
        flags: u32,
        deadline: Deadline,
    ) -> Result<(), Error> {
        send_segments(
            &mut self.stream,
            self.version,
            kind,
            sequence,
            payloads,
            flags,
            deadline,
        )
    }
    /// Borrowed payload remains valid until the next mutable transport call.
    /// Partial timeouts retain all bytes; protocol faults latch the stream closed.
    pub fn receive_view(&mut self, deadline: Deadline) -> Result<(Header, &[u8]), Error> {
        let (header, payload) = self.receive_range(deadline)?;
        Ok((header, &self.received[payload]))
    }
    fn receive_range(&mut self, deadline: Deadline) -> Result<(Header, Range<usize>), Error> {
        if self.desynced {
            return Err(Error::Contract("TCP stream desynced"));
        }
        self.fill(32, deadline)?;
        let bytes: &[u8; 32] = self.received[self.start..self.start + 32]
            .try_into()
            .expect("filled header");
        let header = match Header::decode_version(bytes, self.version) {
            Ok(header) if header.length <= 16 << 20 => header,
            Ok(_) => {
                self.desynced = true;
                return Err(Error::Contract("TCP message cap"));
            }
            Err(error) => {
                self.desynced = true;
                return Err(error);
            }
        };
        let length = header.length as usize;
        let pad = usize::from(header.flags & wire::PADDED != 0);
        self.fill(32 + length + pad, deadline)?;
        let payload = self.start + 32;
        self.start += 32 + length + pad;
        if self.start == self.end {
            self.start = 0;
            self.end = 0;
        }
        Ok((header, payload..payload + length))
    }
}
fn send_segments(
    stream: &mut TcpStream,
    version: Version,
    kind: u16,
    sequence: u32,
    payloads: &[&[u8]],
    mut flags: u32,
    deadline: Deadline,
) -> Result<(), Error> {
    if payloads.len() > 6 {
        return Err(Error::Contract("TCP payload segment cap"));
    }
    let length = payloads
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(p.len()))
        .ok_or(Error::Contract("TCP message cap"))?;
    if length > 16 << 20 {
        return Err(Error::Contract("TCP message cap"));
    }
    let pad = usize::from((32 + length).is_multiple_of(version.packet_multiple()));
    if pad != 0 {
        flags |= wire::PADDED;
    }
    let header = Header {
        kind,
        sequence,
        flags,
        length: length as u32,
        reserved: 0,
    }
    .encode_version(version);
    let padding = [0u8];
    let mut segments: [&[u8]; 8] = [&[]; 8];
    segments[0] = &header;
    segments[1..1 + payloads.len()].copy_from_slice(payloads);
    segments[1 + payloads.len()] = &padding[..pad];
    let parts = &segments[..payloads.len() + 2];
    let mut index = 0;
    let mut offset = 0;
    while index < parts.len() {
        if offset == parts[index].len() {
            index += 1;
            offset = 0;
            continue;
        }
        stream.set_write_timeout(Some(deadline.remaining()?))?;
        let mut vectors: [IoSlice<'_>; 8] = std::array::from_fn(|_| IoSlice::new(&[]));
        vectors[0] = IoSlice::new(&parts[index][offset..]);
        for (out, part) in vectors[1..].iter_mut().zip(&parts[index + 1..]) {
            *out = IoSlice::new(part);
        }
        let mut written = match stream.write_vectored(&vectors[..parts.len() - index]) {
            Ok(0) => return Err(Error::Closed),
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Err(Error::Deadline)
            }
            Err(error) => return Err(error.into()),
        };
        while written > 0 {
            let n = written.min(parts[index].len() - offset);
            offset += n;
            written -= n;
            if offset == parts[index].len() {
                index += 1;
                offset = 0;
            }
        }
    }
    deadline.remaining()?;
    Ok(())
}
impl Transport for TcpTransport {
    fn version(&self) -> Version {
        self.version
    }
    fn gadget(&self) -> bool {
        false
    }
    fn send(&mut self, mut bytes: &[u8], deadline: Deadline) -> Result<(), Error> {
        while !bytes.is_empty() {
            self.stream.set_write_timeout(Some(deadline.remaining()?))?;
            match self.stream.write(bytes) {
                Ok(0) => return Err(Error::Closed),
                Ok(n) => bytes = &bytes[n..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        deadline.remaining()?;
        Ok(())
    }
    fn receive(&mut self, deadline: Deadline) -> Result<Message, Error> {
        let (header, payload) = self.receive_view(deadline)?;
        Ok(Message {
            header,
            payload: payload.to_vec(),
        })
    }

    fn close(&mut self) {
        if let Err(error) = self.stream.shutdown(Shutdown::Both) {
            if error.kind() != std::io::ErrorKind::NotConnected {
                eprintln!("Jetlink TCP close: {error}");
            }
        }
    }
}
impl Drop for TcpTransport {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, time::Duration};
    #[test]
    fn vectored_borrowed_send_matches_actual_source_wire_hashes_without_payload_copy() {
        use serde_json::Value;
        use sha2::{Digest, Sha256};
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/host_stream_source.json"))
                .unwrap();
        for case in fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["fault"] == "none")
        {
            let version = Version::from_u16(case["version"].as_u64().unwrap() as u16).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut hash = Sha256::new();
                let mut buffer = [0; 7919];
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    hash.update(&buffer[..n]);
                }
                format!("{:x}", hash.finalize())
            });
            let mut transport = TcpTransport::connect(
                address,
                version,
                Deadline::after(Duration::from_secs(10)).unwrap(),
            )
            .unwrap();
            for (seq, length) in case["lengths"].as_array().unwrap().iter().enumerate() {
                let payload: Vec<_> = (0..length.as_u64().unwrap() as usize)
                    .map(|i| ((i + seq * 17) % 251) as u8)
                    .collect();
                transport
                    .send_parts(
                        8,
                        seq as u32,
                        &[&payload[..payload.len() / 3], &payload[payload.len() / 3..]],
                        0,
                        Deadline::after(Duration::from_secs(10)).unwrap(),
                    )
                    .unwrap();
            }
            drop(transport);
            assert_eq!(peer.join().unwrap(), case["wire_sha256"].as_str().unwrap());
        }
    }
    #[test]
    fn reusable_stream_matches32_actual_source_fragmentation_padding_and_fault_scenarios() {
        use serde_json::{json, Value};
        use sha2::{Digest, Sha256};
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/host_stream_source.json"))
                .unwrap();
        assert_eq!(
            fixture["source_commit"],
            "e8e152dbcfd9574a1791ed12026d1880e87689b3"
        );
        assert_eq!(
            fixture["base_sha256"],
            "1861db5ec506b274f940797a6d2b181f075e94a928326d699c34cfd6b0c93a3b"
        );
        assert_eq!(fixture["cases"].as_array().unwrap().len(), 32);
        for case in fixture["cases"].as_array().unwrap() {
            let version = Version::from_u16(case["version"].as_u64().unwrap() as u16).unwrap();
            let mut bytes = Vec::new();
            for (seq, length) in case["lengths"].as_array().unwrap().iter().enumerate() {
                let payload: Vec<_> = (0..length.as_u64().unwrap() as usize)
                    .map(|i| ((i + seq * 17) % 251) as u8)
                    .collect();
                bytes.extend(wire::frame_version(8, seq as u32, &payload, false, version).unwrap());
            }
            let fault = case["fault"].as_str().unwrap();
            if fault != "none" {
                let mut header = Header {
                    kind: 8,
                    sequence: 99,
                    flags: 0,
                    length: 0,
                    reserved: 0,
                }
                .encode_version(version);
                match fault {
                    "magic" => header[0] = b'X',
                    "version" => header[4] = 5 - version as u8,
                    "oversize" => header[16..20].copy_from_slice(&16777217u32.to_le_bytes()),
                    _ => unreachable!(),
                }
                bytes.extend(header);
            }
            let chunk = case["chunk"].as_u64().unwrap() as usize;
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                for part in bytes.chunks(chunk) {
                    stream.write_all(part).unwrap();
                }
            });
            let mut transport = TcpTransport::connect(
                address,
                version,
                Deadline::after(Duration::from_secs(10)).unwrap(),
            )
            .unwrap();
            let mut actual = Vec::new();
            for expected in case["expected"].as_array().unwrap() {
                match transport.receive_view(Deadline::after(Duration::from_secs(10)).unwrap()) {
                    Ok((header, payload)) => actual.push(json!({"kind":header.kind,"seq":header.sequence,"flags":header.flags,"len":payload.len(),"sha":format!("{:x}",Sha256::digest(payload))})),
                    Err(Error::Contract("TCP stream desynced")) => actual.push(json!("latched")),
                    Err(_) => actual.push(json!("error")),
                }
                assert_eq!(actual.last().unwrap(), expected, "{case}");
            }
            peer.join().unwrap();
        }
    }
    #[test]
    fn borrowed_receive_retains_partial_body_across_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, ready) = std::sync::mpsc::sync_channel(1);
        let (resume, wait) = std::sync::mpsc::sync_channel(1);
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let bytes = wire::frame_version(8, 7, &[9; 480], false, Version::V3).unwrap();
            stream.write_all(&bytes[..35]).unwrap();
            sent.send(()).unwrap();
            wait.recv().unwrap();
            stream.write_all(&bytes[35..]).unwrap();
            stream
                .write_all(&wire::frame_version(9, 8, b"next", false, Version::V3).unwrap())
                .unwrap();
        });
        let mut transport = TcpTransport::connect(
            address,
            Version::V3,
            Deadline::after(Duration::from_secs(5)).unwrap(),
        )
        .unwrap();
        ready.recv().unwrap();
        assert!(matches!(
            transport.receive_view(Deadline::after(Duration::from_millis(10)).unwrap()),
            Err(Error::Deadline)
        ));
        resume.send(()).unwrap();
        let (header, payload) = transport
            .receive_view(Deadline::after(Duration::from_secs(5)).unwrap())
            .unwrap();
        assert_eq!(header.sequence, 7);
        assert_eq!(header.flags, wire::PADDED);
        assert_eq!(payload, [9; 480]);
        let (header, payload) = transport
            .receive_view(Deadline::after(Duration::from_secs(5)).unwrap())
            .unwrap();
        assert_eq!(header.sequence, 8);
        assert_eq!(payload, b"next");
        peer.join().unwrap();
    }
    #[test]
    fn borrowed_receive_reuses_storage_after_growth_and_latches_protocol_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            for sequence in 0..4 {
                let payload = vec![sequence as u8; (1 << 20) + 480];
                let bytes = wire::frame_version(8, sequence, &payload, false, Version::V3).unwrap();
                for part in bytes.chunks(7919) {
                    stream.write_all(part).unwrap();
                }
            }
            let mut invalid = wire::frame_version(8, 4, b"bad", false, Version::V3).unwrap();
            invalid[0] = b'X';
            stream.write_all(&invalid).unwrap();
            stream
                .write_all(&wire::frame_version(8, 5, b"unusable", false, Version::V3).unwrap())
                .unwrap();
        });
        let mut transport = TcpTransport::connect(
            address,
            Version::V3,
            Deadline::after(Duration::from_secs(5)).unwrap(),
        )
        .unwrap();
        let mut storage = None;
        for sequence in 0..4 {
            let (header, payload) = transport
                .receive_view(Deadline::after(Duration::from_secs(5)).unwrap())
                .unwrap();
            assert_eq!(header.sequence, sequence);
            assert_eq!(payload.len(), (1 << 20) + 480);
            assert!(payload.iter().all(|b| *b == sequence as u8));
            if let Some(pointer) = storage {
                assert_eq!(payload.as_ptr(), pointer);
            }
            storage = Some(payload.as_ptr());
        }
        assert!(transport
            .receive_view(Deadline::after(Duration::from_secs(5)).unwrap())
            .is_err());
        assert!(matches!(
            transport.receive_view(Deadline::after(Duration::from_secs(5)).unwrap()),
            Err(Error::Contract("TCP stream desynced"))
        ));
        peer.join().unwrap();
    }
}
