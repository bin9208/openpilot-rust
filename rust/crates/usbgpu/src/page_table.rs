use crate::Error;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressSpace {
    Physical,
    System,
    Peer,
}
#[derive(Clone, Copy, Debug)]
pub struct PtePolicy {
    pub gfx_major: u8,
    pub uncached_type: u8,
    pub address_mask: u64,
    pub physical_base: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct PteOptions {
    pub table: bool,
    pub uncached: bool,
    pub space: AddressSpace,
    pub snooped: bool,
    pub fragment: u8,
    pub valid: bool,
}
impl Default for PteOptions {
    fn default() -> Self {
        Self {
            table: false,
            uncached: false,
            space: AddressSpace::Physical,
            snooped: false,
            fragment: 0,
            valid: true,
        }
    }
}
impl PtePolicy {
    pub fn flags(self, level: u8, options: PteOptions) -> u64 {
        let mut flags = (u64::from(options.space == AddressSpace::System) * 2)
            | (u64::from(options.snooped) * 4)
            | u64::from(options.valid)
            | (u64::from(options.fragment & 31) << 7);
        if !options.table {
            flags |= 0x70;
        }
        let memory_type = if options.uncached {
            u64::from(self.uncached_type)
        } else {
            0
        };
        if self.gfx_major >= 12 {
            flags |= memory_type << 54;
            if !options.table {
                flags |= 1 << 63;
            }
        } else if self.gfx_major >= 10 {
            flags |= memory_type << 48;
            if !options.table && level != 3 {
                flags |= 1 << 54;
            }
        } else {
            flags |= memory_type << 57;
            if options.table && level == 1 {
                flags |= 9 << 59;
            }
            if options.table && level == 2 {
                flags |= 1 << 56;
            }
            if !options.table && level != 2 && level != 3 {
                flags |= 1 << 54;
            }
        }
        flags
    }
    pub fn entry(self, level: u8, address: u64, options: PteOptions) -> Result<u64, Error> {
        let address = if options.space == AddressSpace::Physical {
            address
                .checked_add(self.physical_base)
                .ok_or(Error::Contract("physical GPU address overflow"))?
        } else {
            address
        };
        if address & self.address_mask != address {
            return Err(Error::Contract("physical GPU address exceeds aperture"));
        }
        Ok(self.flags(level, options) | (address & 0x0000fffffffff000))
    }
    pub fn is_page(self, level: u8, entry: u64) -> bool {
        if level == 3 {
            return true;
        }
        if self.gfx_major < 10 {
            if level == 2 {
                entry & (1 << 56) == 0
            } else {
                entry & (1 << 54) != 0
            }
        } else {
            entry & (1 << if self.gfx_major >= 12 { 63 } else { 54 }) != 0
        }
    }
    pub fn address(self, entry: u64) -> Result<u64, Error> {
        if entry & 2 != 0 {
            return Err(Error::Contract("page table address is system memory"));
        }
        (entry & 0x0000fffffffff000)
            .checked_sub(self.physical_base)
            .ok_or(Error::Contract(
                "page table address below physical GPU base",
            ))
    }
}
pub fn fragment(address: u64, size: u64, must_cover: bool) -> Result<u8, Error> {
    if size == 0 {
        return Err(Error::Contract("zero TLB fragment size"));
    }
    let address_bits = if address == 0 {
        63
    } else {
        address.trailing_zeros()
    };
    let size_bits = if must_cover {
        size.trailing_zeros()
    } else {
        63 - size.leading_zeros()
    };
    let power = address_bits.min(size_bits);
    u8::try_from(
        power
            .checked_sub(12)
            .ok_or(Error::Contract("TLB fragment smaller than 4 KiB"))?,
    )
    .map_err(|_| Error::Contract("TLB fragment overflow"))
}
