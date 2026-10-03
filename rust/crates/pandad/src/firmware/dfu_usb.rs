use super::{Error, Mcu, Request, Transport};

fn request(command: u8, value: u16) -> Request {
    Request {
        timeout_ms: 0,
        ..Request::new(0x21, command, value)
    }
}

pub struct DfuUsb<'a, T> {
    pub transport: &'a mut T,
    pub mcu: Mcu,
}

impl<T: Transport> DfuUsb<'_, T> {
    fn status(&mut self) -> Result<(), Error<T::Error>> {
        loop {
            let reply = self
                .transport
                .control_read(request(3, 0), 6)
                .map_err(Error::Transport)?;
            if *reply.get(1).ok_or(Error::ShortStatus)? == 0 {
                return Ok(());
            }
        }
    }

    fn address(&mut self, command: u8, address: u32) -> Result<(), Error<T::Error>> {
        let mut data = vec![command];
        data.extend_from_slice(&address.to_le_bytes());
        self.transport
            .control_write(request(1, 0), &data)
            .map_err(Error::Transport)?;
        self.status()
    }

    pub fn clear_status(&mut self) -> Result<(), Error<T::Error>> {
        let reply = self
            .transport
            .control_read(request(3, 0), 6)
            .map_err(Error::Transport)?;
        match *reply.get(4).ok_or(Error::ShortStatus)? {
            0x0a => {
                self.transport
                    .control_read(request(4, 0), 0)
                    .map_err(Error::Transport)?;
            }
            0x09 => {
                self.transport
                    .control_write(request(6, 0), &[])
                    .map_err(Error::Transport)?;
                self.status()?;
            }
            _ => {}
        }
        self.transport
            .control_read(request(3, 0), 6)
            .map_err(Error::Transport)?;
        Ok(())
    }

    pub fn erase_sector(&mut self, index: usize) -> Result<(), Error<T::Error>> {
        self.address(0x41, self.mcu.sector_address(index))
    }

    pub fn program(&mut self, address: u32, data: &[u8]) -> Result<(), Error<T::Error>> {
        self.program_with_progress(address, data, |_| Ok(()))
    }

    pub fn program_with_progress(
        &mut self,
        address: u32,
        data: &[u8],
        mut progress: impl FnMut(String) -> Result<(), T::Error>,
    ) -> Result<(), Error<T::Error>> {
        self.address(0x21, address)?;
        let block_size = data.len().min(self.mcu.block_size());
        if block_size == 0 {
            return Err(Error::EmptyBinary);
        }
        for (index, block) in data.chunks(block_size).enumerate() {
            let number = u16::try_from(index + 2).map_err(|_| Error::BlockNumber)?;
            let mut padded = block.to_vec();
            padded.resize(block_size, 0xff);
            progress(format!("programming {index} with length {block_size}"))
                .map_err(Error::Transport)?;
            self.transport
                .control_write(request(1, number), &padded)
                .map_err(Error::Transport)?;
            self.status()?;
        }
        Ok(())
    }

    pub fn jump(&mut self, address: u32) -> Result<(), Error<T::Error>> {
        self.address(0x21, address)?;
        if self.transport.control_write(request(1, 2), &[]).is_ok() {
            let _ = self.transport.control_read(request(3, 0), 6);
        }
        Ok(())
    }

    pub fn recover(&mut self, code: &[u8]) -> Result<(), Error<T::Error>> {
        self.recover_with_progress(code, |_| Ok(()))
    }

    pub fn recover_with_progress(
        &mut self,
        code: &[u8],
        progress: impl FnMut(String) -> Result<(), T::Error>,
    ) -> Result<(), Error<T::Error>> {
        self.clear_status()?;
        for index in 0..self.mcu.sectors().len() {
            self.erase_sector(index)?;
        }
        self.program_with_progress(0x0800_0000, code, progress)?;
        self.jump(0x0800_0000)
    }
}
