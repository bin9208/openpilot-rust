//! Connection-owned RAM input slot, as carrot's `modeld/jetlink/shared_input.py`
//! (`ram_input_v1`).
//!
//! The daemon creates a size-sealed memfd for one modeld connection and hands
//! it over the socket. modeld writes the warped frame and packed inputs into
//! it and sends a small token instead of the frame. One matching reply ends
//! the frame. Any failure retires the whole connection: a late reader still
//! sees the old mapping, and no slot is ever reused by another connection.
use crate::{contract, rpc, Deadline, Error};
use rustix::{
    fd::{AsFd, OwnedFd},
    fs::{MemfdFlags, SealFlags},
    mm::{MapFlags, ProtFlags},
    net::{
        RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer,
        SendAncillaryMessage, SendFlags,
    },
};
use std::{
    io::{IoSlice, IoSliceMut},
    mem::MaybeUninit,
    os::unix::net::UnixStream,
};

/// modeld asks for a slot with this packet right after the handshake.
pub const SETUP: &[u8] = b"JETLINK_RAM_INPUT_V1";
/// Token request kind; the frame request is `b'I'`.
pub const TOKEN_KIND: u8 = b'T';
/// `T` + the 29-byte frame header + nonce, sequence, ready time, source SOF.
pub const TOKEN_BYTES: usize = 1 + rpc::REQUEST_BYTES + 16 + 8 + 8 + 8;
const MAX_BYTES: usize = 1 << 20;
/// carrot rejects a token older than this.
const MAX_AGE_NS: u64 = 150_000_000;

fn seals() -> SealFlags {
    SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL
}

pub fn size_for(packed_count: usize) -> Result<usize, Error> {
    let size = contract::WARPED_BYTES + packed_count * 4;
    if size == 0 || size > MAX_BYTES || !contract::WARPED_BYTES.is_multiple_of(4) {
        return Err(Error::Contract("invalid shared input size"));
    }
    Ok(size)
}

struct Mapping {
    ptr: *mut std::ffi::c_void,
    len: usize,
}
// The mapping is owned by one connection and only touched by its thread.
unsafe impl Send for Mapping {}
impl Mapping {
    fn new(fd: &OwnedFd, len: usize, writable: bool) -> Result<Self, Error> {
        let prot = if writable {
            ProtFlags::READ | ProtFlags::WRITE
        } else {
            ProtFlags::READ
        };
        // SAFETY: a fresh shared mapping of a sealed memfd of exactly `len`
        // bytes; it is unmapped only in Drop.
        let ptr =
            unsafe { rustix::mm::mmap(std::ptr::null_mut(), len, prot, MapFlags::SHARED, fd, 0) }
                .map_err(std::io::Error::from)?;
        Ok(Self { ptr, len })
    }
    fn bytes(&self) -> &[u8] {
        // SAFETY: `ptr` maps `len` readable bytes for the life of `self`.
        unsafe { std::slice::from_raw_parts(self.ptr.cast::<u8>(), self.len) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: writable mapping created by `Writer`, exclusively borrowed.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.cast::<u8>(), self.len) }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: unmaps exactly the region mapped in `new`.
        let _ = unsafe { rustix::mm::munmap(self.ptr, self.len) };
    }
}

fn nonce() -> Result<[u8; 16], Error> {
    let mut value = [0; 16];
    std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom")?, &mut value)?;
    Ok(value)
}

/// Daemon side: owns the read-only view of one connection's slot.
pub struct Reader {
    mapping: Mapping,
    nonce: [u8; 16],
    sequence: u64,
    packed_count: usize,
}
impl Reader {
    /// Creates the slot and sends it to the modeld that asked for it.
    pub fn offer(
        stream: &mut UnixStream,
        packed_count: usize,
        deadline: Deadline,
    ) -> Result<Self, Error> {
        let size = size_for(packed_count)?;
        let fd = rustix::fs::memfd_create(
            "jetlink-input",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )
        .map_err(std::io::Error::from)?;
        rustix::fs::ftruncate(&fd, size as u64).map_err(std::io::Error::from)?;
        rustix::fs::fcntl_add_seals(&fd, seals()).map_err(std::io::Error::from)?;
        // Only modeld writes; the daemon's view cannot change the frame.
        let mapping = Mapping::new(&fd, size, false)?;
        let nonce = nonce()?;
        stream.set_write_timeout(Some(deadline.remaining()?))?;
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        let fds = [fd.as_fd()];
        if !control.push(SendAncillaryMessage::ScmRights(&fds)) {
            return Err(Error::Contract("shared input descriptor buffer"));
        }
        let sent = rustix::net::sendmsg(
            stream.as_fd(),
            &[IoSlice::new(&nonce[..1])],
            &mut control,
            SendFlags::NOSIGNAL,
        )
        .map_err(std::io::Error::from)?;
        if sent != 1 {
            return Err(Error::Contract("shared input descriptor send failed"));
        }
        rpc::send_packet(stream, &nonce, deadline)?;
        Ok(Self {
            mapping,
            nonce,
            sequence: 0,
            packed_count,
        })
    }
    /// Turns a token into the same request bytes a copied frame would carry.
    pub fn read(&mut self, token: &[u8]) -> Result<Vec<u8>, Error> {
        if token.len() != TOKEN_BYTES || token[0] != TOKEN_KIND {
            return Err(Error::Contract("invalid shared input token"));
        }
        let header = &token[1..1 + rpc::REQUEST_BYTES];
        let tail = &token[1 + rpc::REQUEST_BYTES..];
        let word = |at: usize| u64::from_le_bytes(tail[at..at + 8].try_into().expect("8 bytes"));
        let (sequence, ready, source_sof) = (word(16), word(24), word(32));
        let age = crate::now_ns().checked_sub(ready);
        if tail[..16] != self.nonce
            || sequence != self.sequence.wrapping_add(1)
            || !age.is_some_and(|age| age <= MAX_AGE_NS)
        {
            return Err(Error::Contract("stale or foreign shared input"));
        }
        self.sequence = sequence;
        let size = size_for(self.packed_count)?;
        let mut request = Vec::with_capacity(1 + rpc::REQUEST_BYTES + size + 8);
        request.push(b'I');
        request.extend_from_slice(header);
        request.extend_from_slice(&self.mapping.bytes()[..size]);
        if source_sof > 0 {
            request.extend(source_sof.to_le_bytes());
        }
        Ok(request)
    }
}

/// modeld side: owns the writable view of the slot it was given.
pub struct Writer {
    mapping: Mapping,
    nonce: [u8; 16],
    sequence: u64,
    packed_count: usize,
}
impl Writer {
    /// Asks for a slot and receives it. The caller retires the connection on
    /// any error, since the stream position is then unknown.
    pub fn request(
        stream: &mut UnixStream,
        packed_count: usize,
        deadline: Deadline,
    ) -> Result<Self, Error> {
        let size = size_for(packed_count)?;
        rpc::send_packet(stream, SETUP, deadline)?;
        stream.set_read_timeout(Some(deadline.remaining()?))?;
        let mut first = [0u8; 1];
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(2))];
        let mut control = RecvAncillaryBuffer::new(&mut space);
        let received = rustix::net::recvmsg(
            stream.as_fd(),
            &mut [IoSliceMut::new(&mut first)],
            &mut control,
            RecvFlags::CMSG_CLOEXEC,
        )
        .map_err(std::io::Error::from)?;
        let mut fds = Vec::new();
        let mut unknown = false;
        for message in control.drain() {
            match message {
                RecvAncillaryMessage::ScmRights(received) => fds.extend(received),
                _ => unknown = true,
            }
        }
        if received.bytes != 1
            || received
                .flags
                .intersects(rustix::net::ReturnFlags::CTRUNC | rustix::net::ReturnFlags::TRUNC)
            || unknown
            || fds.len() != 1
        {
            return Err(Error::Contract("invalid shared input descriptor"));
        }
        let fd = fds.pop().expect("one descriptor");
        let stat = rustix::fs::fstat(&fd).map_err(std::io::Error::from)?;
        let sealed = rustix::fs::fcntl_get_seals(&fd).map_err(std::io::Error::from)?;
        if usize::try_from(stat.st_size).ok() != Some(size) || !sealed.contains(seals()) {
            return Err(Error::Contract(
                "unsealed or incorrectly sized shared input",
            ));
        }
        let nonce = rpc::recv_packet(stream, deadline)?;
        let nonce: [u8; 16] = nonce
            .try_into()
            .map_err(|_| Error::Contract("invalid shared input session"))?;
        if nonce[0] != first[0] {
            return Err(Error::Contract("invalid shared input session"));
        }
        Ok(Self {
            mapping: Mapping::new(&fd, size, true)?,
            nonce,
            sequence: 0,
            packed_count,
        })
    }
    /// Writes one frame into the slot and returns the token that names it.
    pub fn token(
        &mut self,
        header: &[u8],
        warped: &[u8],
        packed: &[f32],
        source_sof: u64,
    ) -> Result<Vec<u8>, Error> {
        if header.len() != rpc::REQUEST_BYTES
            || warped.len() != contract::WARPED_BYTES
            || packed.len() != self.packed_count
        {
            return Err(Error::Contract("shared input does not fit this frame"));
        }
        let slot = self.mapping.bytes_mut();
        slot[..warped.len()].copy_from_slice(warped);
        for (chunk, value) in slot[warped.len()..].chunks_exact_mut(4).zip(packed) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        self.sequence = self.sequence.wrapping_add(1);
        let mut token = Vec::with_capacity(TOKEN_BYTES);
        token.push(TOKEN_KIND);
        token.extend_from_slice(header);
        token.extend_from_slice(&self.nonce);
        token.extend(self.sequence.to_le_bytes());
        token.extend(crate::now_ns().to_le_bytes());
        token.extend(source_sof.to_le_bytes());
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn token_carries_the_frame_and_rejects_replay_and_foreign_slots() {
        let (mut daemon, mut model) = UnixStream::pair().unwrap();
        let deadline = Deadline::after(Duration::from_secs(2)).unwrap();
        let offer = std::thread::spawn(move || {
            let packet = rpc::recv_packet(&mut daemon, deadline).unwrap();
            assert_eq!(packet, SETUP);
            let reader = Reader::offer(&mut daemon, 12, deadline).unwrap();
            (reader, daemon)
        });
        let mut writer = Writer::request(&mut model, 12, deadline).unwrap();
        let (mut reader, _daemon) = offer.join().unwrap();

        let warped: Vec<u8> = (0..contract::WARPED_BYTES)
            .map(|i| (i % 251) as u8)
            .collect();
        let packed: Vec<f32> = (0..12).map(|i| i as f32 * 0.5).collect();
        let copied = rpc::encode_request([7; 16], 42, Deadline(123), true, &warped, &packed);
        let header = &copied[1..1 + rpc::REQUEST_BYTES];
        let token = writer.token(header, &warped, &packed, 0).unwrap();
        assert_eq!(token.len(), TOKEN_BYTES);
        assert_eq!(reader.read(&token).unwrap(), copied);
        // Same token again is a replay.
        assert!(reader.read(&token).is_err());

        let mut foreign = writer.token(header, &warped, &packed, 9).unwrap();
        foreign[1 + rpc::REQUEST_BYTES] ^= 1;
        assert!(reader.read(&foreign).is_err());
    }
}
