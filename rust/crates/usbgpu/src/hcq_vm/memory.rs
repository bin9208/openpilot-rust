use crate::Error;
use std::collections::BTreeMap;

pub struct Memory {
    blocks: BTreeMap<u64, Vec<u8>>,
    next: u64,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            blocks: BTreeMap::new(),
            next: 1 << 60,
        }
    }
}
impl Memory {
    pub fn allocate(&mut self, bytes: Vec<u8>) -> Result<u64, Error> {
        if bytes.is_empty() {
            return Err(Error::Contract("empty HCQ host allocation"));
        }
        let size =
            u64::try_from(bytes.len()).map_err(|_| Error::Contract("HCQ host size overflow"))?;
        let address = self.next;
        self.next = address
            .checked_add(size)
            .and_then(|end| end.checked_next_multiple_of(4096))
            .and_then(|end| end.checked_add(4096))
            .ok_or(Error::Contract("HCQ host address overflow"))?;
        self.blocks.insert(address, bytes);
        Ok(address)
    }
    pub fn read(&self, address: u64, size: usize) -> Result<&[u8], Error> {
        let (&start, bytes) = self
            .blocks
            .range(..=address)
            .next_back()
            .ok_or(Error::Contract("HCQ read outside owned memory"))?;
        let offset = usize::try_from(address - start)
            .map_err(|_| Error::Contract("HCQ read offset overflow"))?;
        bytes
            .get(
                offset
                    ..offset
                        .checked_add(size)
                        .ok_or(Error::Contract("HCQ read range overflow"))?,
            )
            .ok_or(Error::Contract("HCQ read outside owned memory"))
    }
    pub fn write(&mut self, address: u64, data: &[u8]) -> Result<(), Error> {
        self.read_mut(address, data.len())?.copy_from_slice(data);
        Ok(())
    }
    pub fn read_mut(&mut self, address: u64, size: usize) -> Result<&mut [u8], Error> {
        let (&start, bytes) = self
            .blocks
            .range_mut(..=address)
            .next_back()
            .ok_or(Error::Contract("HCQ write outside owned memory"))?;
        let offset = usize::try_from(address - start)
            .map_err(|_| Error::Contract("HCQ write offset overflow"))?;
        bytes
            .get_mut(
                offset
                    ..offset
                        .checked_add(size)
                        .ok_or(Error::Contract("HCQ write range overflow"))?,
            )
            .ok_or(Error::Contract("HCQ write outside owned memory"))
    }
    pub(super) fn scalar(&self, address: u64, bits: u8) -> Result<u64, Error> {
        let mut value = [0; 8];
        let size = usize::from(bits.div_ceil(8));
        value[..size].copy_from_slice(self.read(address, size)?);
        Ok(u64::from_le_bytes(value))
    }
}
