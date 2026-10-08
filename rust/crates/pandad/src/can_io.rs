use crate::{
    can::{Decoder, EncodeError, Encoder, Frame, ReceiveCapacity, RECEIVE_SIZE},
    device::{Control, Transport},
};

pub trait BulkTransport: Transport {
    fn bulk_read(
        &self,
        endpoint: u8,
        output: &mut [u8],
        timeout_ms: u32,
    ) -> Result<i32, Self::Error>;
    fn bulk_write(&self, endpoint: u8, input: &[u8], timeout_ms: u32) -> Result<i32, Self::Error>;
    fn comms_healthy(&self) -> bool;
}

pub struct Outgoing<'a> {
    pub address: u32,
    pub src: u8,
    pub data: &'a [u8],
}

#[derive(Debug, thiserror::Error)]
pub enum ReceiveError<E> {
    #[error("CAN transport failed: {0}")]
    Transport(E),
    #[error("CAN transport returned invalid receive length {0}")]
    Length(i32),
    #[error(transparent)]
    Capacity(#[from] ReceiveCapacity),
}

pub struct CanIo {
    bus_offset: u32,
    decoder: Decoder,
    maxout: bool,
    receive_buffer: [u8; RECEIVE_SIZE],
}

impl CanIo {
    pub fn new(bus_offset: u32, maxout: bool) -> Self {
        Self {
            bus_offset,
            decoder: Decoder::new(bus_offset),
            maxout,
            receive_buffer: [0; RECEIVE_SIZE],
        }
    }

    pub fn remaining(&self) -> &[u8] {
        self.decoder.remaining()
    }

    pub fn send<T: BulkTransport>(
        &self,
        transport: &T,
        frames: &[Outgoing<'_>],
    ) -> Result<(), EncodeError<T::Error>> {
        let mut encoder = Encoder::new(self.bus_offset);
        let mut write = |bytes: &[u8]| transport.bulk_write(3, bytes, 5).map(|_| ());
        for frame in frames {
            encoder.push(frame.address, frame.src, frame.data, &mut write)?;
        }
        encoder.finish(&mut write)
    }

    pub fn receive<T: BulkTransport>(
        &mut self,
        transport: &T,
        frames: &mut Vec<Frame>,
        mut checksum_failure: impl FnMut(),
    ) -> Result<bool, ReceiveError<T::Error>> {
        let count = transport
            .bulk_read(0x81, &mut self.receive_buffer, 0)
            .map_err(ReceiveError::Transport)?;
        if !transport.comms_healthy() {
            return Ok(false);
        }
        if count > RECEIVE_SIZE as i32 || (self.maxout && count < 0) {
            return Err(ReceiveError::Length(count));
        }
        if self.maxout {
            let mut junk = [0; RECEIVE_SIZE];
            let unused = RECEIVE_SIZE - count as usize;
            transport
                .bulk_read(0xab, &mut junk[..unused], 0)
                .map_err(ReceiveError::Transport)?;
        }
        if count <= 0 {
            return Ok(true);
        }
        let healthy = self
            .decoder
            .push(&self.receive_buffer[..count as usize], frames)?;
        if !healthy {
            checksum_failure();
            transport
                .control_write(Control::new(0xc0, 0, 0))
                .map_err(ReceiveError::Transport)?;
        }
        Ok(healthy)
    }
}

pub const fn send_is_current(received_ns: u64, event_ns: u64, fake_send: bool) -> bool {
    received_ns.saturating_sub(event_ns) < 1_000_000_000 && !fake_send
}
