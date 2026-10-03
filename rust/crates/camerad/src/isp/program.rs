use crate::cdm::{self, Dmi, PackingError};

#[derive(Default, Debug)]
pub struct Program {
    pub bytes: Vec<u8>,
    pub patches: Vec<u32>,
}

impl Program {
    fn extend(&mut self, count: usize) -> Result<&mut [u8], PackingError> {
        let start = self.bytes.len();
        let end = start.checked_add(count).ok_or(PackingError)?;
        self.bytes.resize(end, 0);
        Ok(&mut self.bytes[start..])
    }

    pub fn cont(&mut self, register: u32, values: &[u32]) -> Result<(), PackingError> {
        let count = values
            .len()
            .checked_mul(4)
            .and_then(|size| size.checked_add(8))
            .ok_or(PackingError)?;
        cdm::write_cont(self.extend(count)?, register, values)?;
        Ok(())
    }

    pub fn random(&mut self, values: &[u32]) -> Result<(), PackingError> {
        let count = values
            .len()
            .checked_mul(4)
            .and_then(|size| size.checked_add(4))
            .ok_or(PackingError)?;
        cdm::write_random(self.extend(count)?, values)?;
        Ok(())
    }

    pub fn dmi(&mut self, bytes: u32, register: u32, selector: u8) -> Result<(), PackingError> {
        self.dmi_opcode(bytes, register, selector, 10)
    }

    pub fn dmi_opcode(
        &mut self,
        bytes: u32,
        register: u32,
        selector: u8,
        opcode: u8,
    ) -> Result<(), PackingError> {
        let start = self.bytes.len();
        let offset = cdm::write_dmi(
            self.extend(12)?,
            Dmi {
                length: bytes,
                address: register,
                selector,
                opcode,
            },
        )?;
        self.patches
            .push(u32::try_from(start + offset).map_err(|_| PackingError)?);
        Ok(())
    }

    pub fn yuv(&mut self, ife: bool) -> Result<(), PackingError> {
        self.cont(
            if ife { 0xf30 } else { 0x3468 },
            &[
                0x00680208, 0x00000108, 0x00400000, 0x03ff0000, 0x01c01ed8, 0x00001f68, 0x02000000,
                0x03ff0000, 0x1fb81e88, 0x000001c0, 0x02000000, 0x03ff0000,
            ],
        )
    }
}
