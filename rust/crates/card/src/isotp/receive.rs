use super::{CanIo, Error, IsoTpMessage};

impl IsoTpMessage {
    pub(super) fn receive_next(
        &mut self,
        data: &[u8],
        io: &mut impl CanIo,
    ) -> Result<FrameType, Error> {
        let first = *data.first().ok_or(Error::Truncated)?;
        match first >> 4 {
            0 => {
                if !self.rx_dat.is_empty() && !self.rx_done {
                    return Err(Error::SingleActive);
                }
                let offset = if first & 15 == 0 && data.len() > 8 {
                    self.rx_len = usize::from(*data.get(1).ok_or(Error::Truncated)?);
                    if self.rx_len
                        > if self.client.sub_addr.is_some() {
                            61
                        } else {
                            62
                        }
                    {
                        return Err(Error::SingleLength(self.rx_len));
                    }
                    2
                } else {
                    self.rx_len = usize::from(first & 15);
                    if self.rx_len >= self.max_len {
                        return Err(Error::SingleLength(self.rx_len));
                    }
                    1
                };
                self.rx_dat = data[offset..data.len().min(offset + self.rx_len)].to_vec();
                self.rx_idx = 0;
                self.rx_done = true;
                Ok(FrameType::Single)
            }
            1 => {
                if !self.rx_dat.is_empty() && !self.rx_done {
                    return Err(Error::FirstActive);
                }
                self.rx_len = usize::from(first & 15) * 256
                    + usize::from(*data.get(1).ok_or(Error::Truncated)?);
                if self.rx_len < self.max_len {
                    return Err(Error::FirstLength(self.rx_len));
                }
                if data.len() != self.max_len {
                    return Err(Error::FirstFrameLength(data.len()));
                }
                self.rx_dat = data[2..].to_vec();
                self.rx_idx = 0;
                self.rx_done = false;
                self.client
                    .send(std::slice::from_ref(&self.flow_control_msg), 0., io)?;
                Ok(FrameType::First)
            }
            2 => {
                if self.rx_done {
                    return Err(Error::ConsecutiveInactive);
                }
                self.rx_idx += 1;
                if self.rx_idx & 15 != usize::from(first & 15) {
                    return Err(Error::ConsecutiveIndex);
                }
                let remaining = self.rx_len.saturating_sub(self.rx_dat.len());
                self.rx_dat
                    .extend_from_slice(&data[1..data.len().min(1 + remaining)]);
                if self.rx_len == self.rx_dat.len() {
                    self.rx_done = true;
                } else if self.single_frame_mode {
                    self.client
                        .send(std::slice::from_ref(&self.flow_control_msg), 0., io)?;
                }
                Ok(FrameType::Consecutive)
            }
            3 => {
                if self.tx_done {
                    return Err(Error::FlowInactive);
                }
                if first == 0x32 {
                    return Err(Error::FlowOverflow);
                }
                if first != 0x30 && first != 0x31 {
                    return Err(Error::FlowIndicator);
                }
                if first == 0x30 {
                    let separation = *data.get(2).ok_or(Error::Truncated)?;
                    let delay = f64::from(separation & 0x7f)
                        / if separation & 0x80 == 0 {
                            1000.
                        } else {
                            10000.
                        };
                    let size = self.max_len - 1;
                    let start = self.max_len - 2 + self.tx_idx * size;
                    let count = usize::from(*data.get(1).ok_or(Error::Truncated)?);
                    let end = if count > 0 {
                        start + count * size
                    } else {
                        self.tx_len
                    };
                    let mut messages = Vec::new();
                    for offset in (start..end).step_by(size) {
                        self.tx_idx += 1;
                        let mut message = vec![
                            0x20 | u8::try_from(self.tx_idx & 15).map_err(|_| Error::TxLength)?,
                        ];
                        if offset < self.tx_dat.len() {
                            message.extend_from_slice(
                                &self.tx_dat[offset..self.tx_dat.len().min(offset + size)],
                            );
                        }
                        message.resize(self.max_len, 0);
                        messages.push(message);
                    }
                    self.client.send(&messages, delay, io)?;
                    if end >= self.tx_len {
                        self.tx_done = true;
                    }
                }
                Ok(FrameType::Flow)
            }
            kind => Err(Error::FrameType(kind)),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum FrameType {
    Single,
    First,
    Consecutive,
    Flow,
}
