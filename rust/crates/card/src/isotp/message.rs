use super::receive::FrameType;
use super::{CanClient, CanIo, Error};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct IsoTpMessage {
    pub client: CanClient,
    pub timeout: f64,
    pub single_frame_mode: bool,
    pub max_len: usize,
    pub flow_control_msg: Vec<u8>,
    pub tx_dat: Vec<u8>,
    pub tx_len: usize,
    pub tx_idx: usize,
    pub tx_done: bool,
    pub rx_dat: Vec<u8>,
    pub rx_len: usize,
    pub rx_idx: usize,
    pub rx_done: bool,
}
impl IsoTpMessage {
    /// Creates source-compatible ISO-TP state, including its separation-time encoding.
    /// # Errors
    /// Returns [`Error::SeparationTime`] for unsupported separation times.
    pub fn new(
        client: CanClient,
        timeout: f64,
        single_frame_mode: bool,
        separation_time: f64,
    ) -> Result<Self, Error> {
        let encoded = if (1e-4..=9e-4).contains(&separation_time) {
            // Decimal round to four places before the source's integer conversion.
            let decimal: f64 = format!("{separation_time:.4}")
                .parse()
                .map_err(|_| Error::SeparationTime)?;
            u8::try_from((decimal * 1e4).trunc() as i32).map_err(|_| Error::SeparationTime)? + 0xf0
        } else if (0.0..=0.127).contains(&separation_time) {
            u8::try_from((separation_time * 1000.).round_ties_even() as i32)
                .map_err(|_| Error::SeparationTime)?
        } else {
            return Err(Error::SeparationTime);
        };
        let max_len = if client.sub_addr.is_some() { 7 } else { 8 };
        let mut flow_control_msg = vec![0x30, u8::from(single_frame_mode), encoded];
        flow_control_msg.resize(max_len, 0);
        Ok(Self {
            client,
            timeout,
            single_frame_mode,
            max_len,
            flow_control_msg,
            tx_dat: Vec::new(),
            tx_len: 0,
            tx_idx: 0,
            tx_done: false,
            rx_dat: Vec::new(),
            rx_len: 0,
            rx_idx: 0,
            rx_done: false,
        })
    }
    /// Starts a new query. `setup_only` records a functional request without physical transmission.
    /// # Errors
    /// Returns [`Error::TxLength`] or transport errors.
    pub fn send(
        &mut self,
        data: &[u8],
        setup_only: bool,
        io: &mut impl CanIo,
    ) -> Result<(), Error> {
        // Source calls the recv(drain=True) generator without iterating it. It does not drain.
        self.tx_dat.clear();
        self.tx_dat.extend_from_slice(data);
        self.tx_len = data.len();
        self.tx_idx = 0;
        self.tx_done = false;
        self.rx_dat.clear();
        self.rx_len = 0;
        self.rx_idx = 0;
        self.rx_done = false;
        let mut message = Vec::with_capacity(self.max_len);
        if self.tx_len < self.max_len {
            message.push(u8::try_from(self.tx_len).map_err(|_| Error::TxLength)?);
            message.extend_from_slice(data);
            message.resize(self.max_len, 0);
            self.tx_done = true;
        } else {
            let length = u16::try_from(self.tx_len).map_err(|_| Error::TxLength)?;
            message.extend_from_slice(&(0x1000 | length).to_be_bytes());
            message.extend_from_slice(&data[..self.max_len - 2]);
        }
        if !setup_only {
            self.client.send(&[message], 0., io)?;
        }
        Ok(())
    }
    /// Receives one completed response or the source nonblocking in-progress status.
    /// # Errors
    /// Returns [`Error::Timeout`], malformed-frame errors or transport errors.
    pub fn recv(
        &mut self,
        timeout: Option<f64>,
        io: &mut impl CanIo,
    ) -> Result<(Option<Vec<u8>>, bool), Error> {
        let timeout = timeout.unwrap_or(self.timeout);
        let mut start = io.now();
        let mut in_progress = false;
        loop {
            self.client.receive_buffer(false, io)?;
            while let Some(data) = self.client.rx_buff.pop_front() {
                let frame_type = self.receive_next(&data, io)?;
                start = io.now();
                in_progress = matches!(frame_type, FrameType::First | FrameType::Consecutive);
                if self.tx_done && self.rx_done {
                    return Ok((Some(self.rx_dat.clone()), false));
                }
            }
            if timeout == 0. {
                return Ok((None, in_progress));
            }
            if io.now() - start > timeout {
                return Err(Error::Timeout);
            }
        }
    }
}
