mod io;
pub use io::{IoConfig, Plane};

pub const PACKET_SIZE: usize = 64;
pub const PAYLOAD_OFFSET: usize = 56;
pub const COMMAND_SIZE: usize = 24;
pub const IO_SIZE: usize = 256;
pub const PATCH_SIZE: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum PacketError {
    #[error("camera packet size exceeds its 32-bit ABI")]
    Size,
    #[error("camera packet {kind} index {index} is outside {count} entries")]
    Index {
        kind: &'static str,
        index: usize,
        count: usize,
    },
    #[error(transparent)]
    Sensor(#[from] crate::sensor::SensorError),
}

pub(crate) fn u16_at(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn u64_at(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CommandBuffer {
    pub handle: i32,
    pub offset: u32,
    pub size: u32,
    pub length: u32,
    pub kind: u32,
    pub metadata: u32,
}

impl CommandBuffer {
    pub fn bytes(self) -> [u8; COMMAND_SIZE] {
        let words = [
            self.handle as u32,
            self.offset,
            self.size,
            self.length,
            self.kind,
            self.metadata,
        ];
        let mut bytes = [0; COMMAND_SIZE];
        for (index, value) in words.into_iter().enumerate() {
            u32_at(&mut bytes, index * 4, value);
        }
        bytes
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub destination: i32,
    pub destination_offset: u32,
    pub source: i32,
    pub source_offset: u32,
}

#[derive(Debug)]
pub struct Packet {
    bytes: Vec<u8>,
    command_count: usize,
    io_count: usize,
    patch_capacity: usize,
    patch_count: usize,
}

impl Packet {
    pub fn new(
        opcode: u32,
        request: u64,
        commands: usize,
        io: usize,
        patches: usize,
    ) -> Result<Self, PacketError> {
        let size = commands
            .checked_mul(COMMAND_SIZE)
            .and_then(|size| size.checked_add(io.checked_mul(IO_SIZE)?))
            .and_then(|size| size.checked_add(patches.checked_mul(PATCH_SIZE)?))
            .and_then(|size| size.checked_add(PACKET_SIZE))
            .filter(|size| *size <= u32::MAX as usize)
            .ok_or(PacketError::Size)?;
        let mut bytes = vec![0; size];
        u32_at(&mut bytes, 0, opcode);
        u32_at(&mut bytes, 4, size as u32);
        u64_at(&mut bytes, 8, request);
        u32_at(&mut bytes, 28, commands as u32);
        u32_at(
            &mut bytes,
            32,
            if io > 0 {
                (commands * COMMAND_SIZE) as u32
            } else {
                0
            },
        );
        u32_at(&mut bytes, 36, io as u32);
        u32_at(
            &mut bytes,
            40,
            if patches > 0 {
                (commands * COMMAND_SIZE + io * IO_SIZE) as u32
            } else {
                0
            },
        );
        u32_at(&mut bytes, 48, u32::MAX);
        Ok(Self {
            bytes,
            command_count: commands,
            io_count: io,
            patch_capacity: patches,
            patch_count: 0,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn kmd(&mut self, command_index: u32, offset: u32) {
        u32_at(&mut self.bytes, 48, command_index);
        u32_at(&mut self.bytes, 52, offset);
    }

    pub fn command(&mut self, index: usize, command: CommandBuffer) -> Result<(), PacketError> {
        if index >= self.command_count {
            return Err(PacketError::Index {
                kind: "command",
                index,
                count: self.command_count,
            });
        }
        let offset = PAYLOAD_OFFSET + index * COMMAND_SIZE;
        self.bytes[offset..offset + COMMAND_SIZE].copy_from_slice(&command.bytes());
        Ok(())
    }

    pub fn io(&mut self, index: usize, config: IoConfig) -> Result<(), PacketError> {
        if index >= self.io_count {
            return Err(PacketError::Index {
                kind: "I/O",
                index,
                count: self.io_count,
            });
        }
        let offset = PAYLOAD_OFFSET + self.command_count * COMMAND_SIZE + index * IO_SIZE;
        self.bytes[offset..offset + IO_SIZE].copy_from_slice(&config.bytes());
        Ok(())
    }

    pub fn patch(&mut self, patch: Patch) -> Result<(), PacketError> {
        if self.patch_count == self.patch_capacity {
            return Err(PacketError::Index {
                kind: "patch",
                index: self.patch_count,
                count: self.patch_capacity,
            });
        }
        let offset = PAYLOAD_OFFSET
            + self.command_count * COMMAND_SIZE
            + self.io_count * IO_SIZE
            + self.patch_count * PATCH_SIZE;
        let words = [
            patch.destination as u32,
            patch.destination_offset,
            patch.source as u32,
            patch.source_offset,
        ];
        for (index, value) in words.into_iter().enumerate() {
            u32_at(&mut self.bytes, offset + index * 4, value);
        }
        self.patch_count += 1;
        u32_at(&mut self.bytes, 44, self.patch_count as u32);
        Ok(())
    }
}
