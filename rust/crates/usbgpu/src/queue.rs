use crate::{
    packets::{packet3, words64},
    Error,
};

pub trait RingIo {
    fn write_word(&mut self, offset: usize, value: u32) -> Result<(), Error>;
    fn write_bytes(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error>;
    fn write_pointer(&mut self, value: u64) -> Result<(), Error>;
    fn memory_barrier(&mut self) -> Result<(), Error>;
    fn doorbell(&mut self, value: u64) -> Result<(), Error>;
}
#[derive(Clone, Debug)]
pub struct Ring {
    pub virtual_address: u64,
    pub bytes: usize,
    pub put: u64,
}
impl Ring {
    fn valid(&self) -> Result<(), Error> {
        if self.bytes == 0 || !self.bytes.is_multiple_of(4) {
            return Err(Error::Contract("invalid GPU ring size"));
        }
        Ok(())
    }
    fn signal(&self, io: &mut impl RingIo, doorbell: u64) -> Result<(), Error> {
        io.write_pointer(self.put)?;
        io.memory_barrier()?;
        io.doorbell(doorbell)
    }
    pub fn submit_compute(
        &mut self,
        io: &mut impl RingIo,
        commands: &[u32],
        multi_xcc: bool,
        bound: bool,
    ) -> Result<(), Error> {
        self.valid()?;
        let words = self.bytes / 4;
        let indirect;
        let commands = if multi_xcc && !bound {
            let position = (self.put % words as u64) as usize;
            let start = (position + 5) % words;
            let padding = if start + commands.len() > words {
                words - start
            } else {
                0
            };
            let address = self.virtual_address + (((position + 5 + padding) % words) * 4) as u64;
            let pair = words64(address);
            let mut values = vec![
                packet3(0x3f, 2),
                pair[0],
                pair[1],
                commands.len() as u32 | (1 << 23),
                packet3(0x10, (padding + commands.len()).wrapping_sub(1)),
            ];
            values.resize(5 + padding, 0);
            values.extend_from_slice(commands);
            indirect = values;
            &indirect
        } else {
            commands
        };
        for (index, value) in commands.iter().copied().enumerate() {
            io.write_word(
                (((self.put % words as u64) as usize + index) % words) * 4,
                value,
            )?;
        }
        self.put = self
            .put
            .checked_add(commands.len() as u64)
            .ok_or(Error::Contract("compute ring pointer overflow"))?;
        self.signal(io, self.put)
    }
    pub fn submit_copy(
        &mut self,
        io: &mut impl RingIo,
        commands: &[u32],
        sizes: &[usize],
        bound: bool,
    ) -> Result<(), Error> {
        self.valid()?;
        if !self.put.is_multiple_of(4) {
            return Err(Error::Contract("unaligned SDMA ring pointer"));
        }
        let initial = self.put as usize % self.bytes;
        let aligned;
        let bound_sizes;
        let (commands, sizes) = if bound {
            let add = (8 - ((self.put as usize % 32) / 4 + commands.len() % 8) % 8) % 8;
            let add = if (commands.len() + add) * 4 >= self.bytes - initial {
                2
            } else {
                add
            };
            let mut values = vec![0; add];
            values.extend_from_slice(commands);
            bound_sizes = [values.len()];
            aligned = values;
            (aligned.as_slice(), bound_sizes.as_slice())
        } else {
            (commands, sizes)
        };
        if sizes.iter().try_fold(0usize, |a, &b| a.checked_add(b)) != Some(commands.len()) {
            return Err(Error::Contract("SDMA command sizes do not match words"));
        }
        let mut tail = 0;
        for &size in sizes {
            if (tail + size) * 4 >= self.bytes - initial {
                break;
            }
            tail += size;
        }
        let remaining = commands.len() - tail;
        if remaining > 0 {
            tail = 0;
        }
        let total = if remaining == 0 {
            tail * 4
        } else {
            (self.bytes - initial) % self.bytes
        } + remaining * 4;
        if total >= self.bytes {
            return Err(Error::Contract("SDMA queue overrun"));
        }
        let bytes = commands[..tail]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        io.write_bytes(initial, &bytes)?;
        self.put += (tail * 4) as u64;
        if commands.len() > tail {
            let offset = self.put as usize % self.bytes;
            let padding = self.bytes - offset;
            io.write_bytes(offset, &vec![0; padding])?;
            self.put += padding as u64;
            let bytes = commands[tail..]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect::<Vec<_>>();
            io.write_bytes(0, &bytes)?;
            self.put += bytes.len() as u64;
        }
        self.signal(io, self.put)
    }
    pub fn submit_aql(&mut self, io: &mut impl RingIo, packets: &[u8]) -> Result<(), Error> {
        self.valid()?;
        if !packets.len().is_multiple_of(64)
            || packets.len() >= self.bytes
            || !self.bytes.is_multiple_of(64)
        {
            return Err(Error::Contract("invalid AQL submission size"));
        }
        let offset = (self.put % (self.bytes / 64) as u64) as usize * 64;
        let head = packets.len().min(self.bytes - offset);
        io.write_bytes(offset, &packets[..head])?;
        if head < packets.len() {
            io.write_bytes(0, &packets[head..])?;
        }
        self.put += (packets.len() / 64) as u64;
        let doorbell = self
            .put
            .checked_sub(1)
            .ok_or(Error::Contract("empty initial AQL submission"))?;
        self.signal(io, doorbell)
    }
}
