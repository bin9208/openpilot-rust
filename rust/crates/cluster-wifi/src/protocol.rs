use crate::{Error, Registration, ACK, HEADER_BYTES, HELLO, MAGIC, MAX_PAYLOAD};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::io::{self, Read, Write};

type Hmac256 = Hmac<Sha256>;
pub fn mac(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut mac = Hmac256::new_from_slice(key).expect("HMAC accepts a 32-byte key");
    mac.update(message);
    mac.finalize().into_bytes().into()
}
fn authenticated(key: &[u8; 32], message: &[u8], signature: &[u8]) -> bool {
    let mut mac = Hmac256::new_from_slice(key).expect("HMAC accepts a 32-byte key");
    mac.update(message);
    mac.verify_slice(signature).is_ok()
}
pub trait Socket: Read + Write {
    fn nodelay(&mut self) -> io::Result<()>;
    fn close(&mut self) -> io::Result<()>;
}
fn receive(stream: &mut impl Read, size: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            Error::Disconnected
        } else {
            error.into()
        }
    })?;
    Ok(bytes)
}
fn header(values: [u32; 6]) -> [u8; HEADER_BYTES] {
    let mut result = [0; HEADER_BYTES];
    for (slot, value) in result.chunks_exact_mut(4).zip(values) {
        slot.copy_from_slice(&value.to_be_bytes());
    }
    result
}
pub struct Connection<S: Socket> {
    socket: Option<S>,
    key: [u8; 32],
    sequence: u64,
}
impl<S: Socket> Connection<S> {
    /// Socket is already connected with the source's three-second IO timeout.
    pub fn establish(
        mut socket: S,
        registration: &Registration,
        nonce: impl FnOnce() -> Result<[u8; 32], Error>,
    ) -> Result<Self, Error> {
        socket.nodelay()?;
        let result = (|| {
            let hello = receive(&mut socket, 42)?;
            if &hello[..4] != HELLO || hello[4..10] != registration.device {
                return Err(Error::Identity);
            }
            let nonce = nonce()?;
            let mut message = b"session".to_vec();
            message.extend(&hello);
            message.extend(nonce);
            let key = mac(&registration.key, &message);
            let mut message = b"client".to_vec();
            message.extend(&hello);
            message.extend(nonce);
            let mut request = nonce.to_vec();
            request.extend(mac(&registration.key, &message));
            socket.write_all(&request)?;
            let signature = receive(&mut socket, 32)?;
            let mut message = b"server".to_vec();
            message.extend(&hello);
            message.extend(nonce);
            if !authenticated(&registration.key, &message, &signature) {
                return Err(Error::Authentication);
            }
            Ok(key)
        })();
        match result {
            Ok(key) => Ok(Self {
                socket: Some(socket),
                key,
                sequence: 0,
            }),
            Err(error) => {
                socket.close()?;
                Err(error)
            }
        }
    }
    /// Adopt an authenticated stream; the display adapter owns ACK sequencing.
    pub fn authenticated(socket: S, key: [u8; 32], sequence: u64) -> Self {
        Self {
            socket: Some(socket),
            key,
            sequence,
        }
    }
    pub fn session_key(&self) -> &[u8; 32] {
        &self.key
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn send(&mut self, payload: &[u8], expect_reply: bool) -> Result<u64, Error> {
        if !(512..=MAX_PAYLOAD).contains(&payload.len()) {
            return Err(Error::Payload);
        }
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Sequence)?;
        let seq = u32::try_from(self.sequence).map_err(|_| Error::Sequence)?;
        let head = header([
            MAGIC,
            seq,
            u32::try_from(payload.len()).map_err(|_| Error::Payload)?,
            u32::from(expect_reply),
            0,
            0,
        ]);
        let mut signed = head.to_vec();
        signed.extend(payload);
        let mut wire = head.to_vec();
        wire.extend(mac(&self.key, &signed));
        wire.extend(payload);
        self.socket
            .as_mut()
            .ok_or(Error::Disconnected)?
            .write_all(&wire)?;
        Ok(self.sequence)
    }
    pub fn result(&mut self) -> Result<(u32, Vec<u8>, u32), Error> {
        let socket = self.socket.as_mut().ok_or(Error::Disconnected)?;
        let head = receive(socket, HEADER_BYTES)?;
        let fields: Vec<_> = head
            .chunks_exact(4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .collect();
        let (magic, seq, size, status, usb_us, reserved) = (
            fields[0], fields[1], fields[2], fields[3], fields[4], fields[5],
        );
        if magic != ACK || size > 512 || reserved != 0 {
            return Err(Error::Acknowledgement);
        }
        let signature = receive(socket, 32)?;
        let response = receive(socket, size as usize)?;
        let mut message = head;
        message.extend(&response);
        if !authenticated(&self.key, &message, &signature) {
            return Err(Error::Signature);
        }
        if status != 0 {
            return Err(Error::Usb(status));
        }
        Ok((seq, response, usb_us))
    }
    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(mut socket) = self.socket.take() {
            socket.close()?;
        }
        Ok(())
    }
}
impl<S: Socket> Drop for Connection<S> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
