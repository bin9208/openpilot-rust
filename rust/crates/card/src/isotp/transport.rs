use openpilot_can::Frame;
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid tx_addr: {0}")]
    Address(u32),
    #[error("Separation time not in range")]
    SeparationTime,
    #[error("isotp - rx: invalid sub-address: {actual}, expected: {expected}")]
    SubAddress { actual: u8, expected: u8 },
    #[error("isotp - rx: single frame with active frame")]
    SingleActive,
    #[error("isotp - rx: first frame with active frame")]
    FirstActive,
    #[error("isotp - rx: invalid single frame length: {0}")]
    SingleLength(usize),
    #[error("isotp - rx: invalid first frame length: {0}")]
    FirstLength(usize),
    #[error("isotp - rx: invalid CAN frame length: {0}")]
    FirstFrameLength(usize),
    #[error("isotp - rx: consecutive frame with no active frame")]
    ConsecutiveInactive,
    #[error("isotp - rx: invalid consecutive frame index")]
    ConsecutiveIndex,
    #[error("isotp - rx: flow control with no active frame")]
    FlowInactive,
    #[error("isotp - rx: flow-control overflow/abort")]
    FlowOverflow,
    #[error("isotp - rx: flow-control transfer state indicator invalid")]
    FlowIndicator,
    #[error("isotp - rx: invalid frame type: {0}")]
    FrameType(u8),
    #[error("index out of range")]
    Truncated,
    #[error("ushort format requires 0 <= number <= 65535")]
    TxLength,
    #[error("outgoing CAN diagnostic frame exceeds eight bytes")]
    CanLength,
    #[error("timeout waiting for response")]
    Timeout,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
impl Error {
    /// Original source exception class, for unchanged-source differential checks.
    pub const fn source_exception(&self) -> &'static str {
        match self {
            Self::Address(_) => "ValueError",
            Self::SubAddress { .. } => "InvalidSubAddressError",
            Self::Timeout => "MessageTimeoutError",
            Self::Truncated => "IndexError",
            Self::TxLength => "error",
            Self::SeparationTime | Self::FrameType(_) => "Exception",
            Self::SingleActive
            | Self::FirstActive
            | Self::SingleLength(_)
            | Self::FirstLength(_)
            | Self::FirstFrameLength(_)
            | Self::ConsecutiveInactive
            | Self::ConsecutiveIndex
            | Self::FlowInactive
            | Self::FlowOverflow
            | Self::FlowIndicator
            | Self::CanLength => "AssertionError",
            Self::Io(_) => "OSError",
        }
    }
}

/// Transport boundary used by physical, IPC and original-source oracle adapters.
/// `receive` returns a single source CanClient batch; it may be empty.
pub trait CanIo {
    fn receive(&mut self) -> Result<Vec<Frame>, Error>;
    fn send(&mut self, frame: Frame) -> Result<(), Error>;
    fn sleep(&mut self, seconds: f64) -> Result<(), Error>;
    fn now(&mut self) -> f64;
}

/// Physical response address, or no fixed address for functional queries.
/// # Errors
/// Returns [`Error::Address`] for addresses rejected by the source.
pub fn rx_address(tx: u32, offset: i64) -> Result<Option<i64>, Error> {
    if [0x7df, 0x18db33f1].contains(&tx) {
        Ok(None)
    } else if tx < 0xfff8 {
        i64::from(tx)
            .checked_add(offset)
            .map(Some)
            .ok_or(Error::Address(tx))
    } else if tx > 0x10000000 && tx < u32::MAX {
        Ok(Some(i64::from(
            (tx & 0xffff0000) + ((tx << 8) & 0xff00) + ((tx >> 8) & 0xff),
        )))
    } else {
        Err(Error::Address(tx))
    }
}

#[derive(Debug, Serialize)]
pub struct CanClient {
    pub tx_addr: u32,
    pub rx_addr: Option<i64>,
    pub bus: u8,
    pub sub_addr: Option<u8>,
    pub rx_sub_addr: Option<u8>,
    pub rx_buff: VecDeque<Vec<u8>>,
}
impl CanClient {
    pub fn new(
        tx_addr: u32,
        rx_addr: Option<i64>,
        bus: u8,
        sub_addr: Option<u8>,
        rx_sub_addr: Option<u8>,
    ) -> Self {
        Self {
            tx_addr,
            rx_addr,
            bus,
            sub_addr,
            rx_sub_addr: rx_sub_addr.or(sub_addr),
            rx_buff: VecDeque::new(),
        }
    }
    fn accepts(&mut self, bus: u8, addr: u32) -> bool {
        if self.tx_addr == 0x7df {
            let response = (0x7e8..=0x7ef).contains(&addr);
            if response {
                self.tx_addr = addr - 8;
                self.rx_addr = Some(i64::from(addr));
            }
            // The source standard functional branch intentionally does not test bus.
            return response;
        }
        if self.tx_addr == 0x18db33f1 && (0x18daf100..=0x18daf1ff).contains(&addr) {
            self.tx_addr = 0x18da00f1 + ((addr << 8) & 0xff00);
            self.rx_addr = Some(i64::from(addr));
        }
        bus == self.bus && Some(i64::from(addr)) == self.rx_addr
    }
    /// Buffers a complete source batch, including the source 254-frame continuation.
    /// # Errors
    /// Returns transport errors or [`Error::SubAddress`] for a mismatched response.
    pub fn receive_buffer(&mut self, drain: bool, io: &mut impl CanIo) -> Result<(), Error> {
        loop {
            let frames = io.receive()?;
            let length = frames.len();
            if drain {
                self.rx_buff.clear();
            } else {
                for frame in frames {
                    if self.accepts(frame.bus, frame.address) && !frame.data.is_empty() {
                        let payload = if let Some(expected) = self.rx_sub_addr {
                            let actual = frame.data[0];
                            if actual != expected {
                                return Err(Error::SubAddress { actual, expected });
                            }
                            frame.data[1..].to_vec()
                        } else {
                            frame.data
                        };
                        self.rx_buff.push_back(payload);
                    }
                }
            }
            if length < 254 {
                return Ok(());
            }
        }
    }
    /// Sends frames with source delay and receive-buffer servicing cadence.
    /// # Errors
    /// Returns transport/subaddress errors or [`Error::CanLength`] for oversized CAN frames.
    pub fn send(
        &mut self,
        messages: &[Vec<u8>],
        delay: f64,
        io: &mut impl CanIo,
    ) -> Result<(), Error> {
        for (index, data) in messages.iter().enumerate() {
            if delay != 0. && index != 0 {
                io.sleep(delay)?;
            }
            let mut payload = Vec::with_capacity(data.len() + usize::from(self.sub_addr.is_some()));
            if let Some(address) = self.sub_addr {
                payload.push(address);
            }
            payload.extend_from_slice(data);
            if payload.len() > 8 {
                return Err(Error::CanLength);
            }
            io.send(Frame {
                address: self.tx_addr,
                data: payload,
                bus: self.bus,
            })?;
            if index % 10 == 9 {
                self.receive_buffer(false, io)?;
            }
        }
        Ok(())
    }
}
