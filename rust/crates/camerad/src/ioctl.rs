use crate::packet::{u32_at, u64_at};

pub const CONTROL_IOCTL: u64 = 0xc018_56c0;
pub const DEQUEUE_EVENT_IOCTL: u64 = 0x8088_5659;
pub const SUBSCRIBE_EVENT_IOCTL: u64 = 0x4020_565a;

#[derive(Clone, Copy, Debug)]
pub struct SyscallResult {
    pub code: i32,
    pub errno: i32,
}

pub fn retry_interrupted(mut call: impl FnMut() -> SyscallResult) -> SyscallResult {
    for _ in 0..100 {
        let result = call();
        if result.code != -1 || result.errno != 4 {
            return result;
        }
    }
    call()
}

#[derive(Debug)]
#[repr(align(8))]
pub struct ControlEnvelope {
    bytes: [u8; 24],
    sync: bool,
}

impl ControlEnvelope {
    pub fn camera(opcode: u32, handle: u64, size: i32) -> Self {
        let mut bytes = [0; 24];
        u32_at(&mut bytes, 0, opcode);
        u32_at(&mut bytes, 4, if size == 0 { 8 } else { size as u32 });
        u32_at(&mut bytes, 8, if size == 0 { 2 } else { 1 });
        u64_at(&mut bytes, 16, handle);
        Self { bytes, sync: false }
    }

    pub fn sync(id: u32, pointer: u64, size: u32) -> Self {
        let mut bytes = [0; 24];
        u32_at(&mut bytes, 0, id);
        u32_at(&mut bytes, 4, size);
        u64_at(&mut bytes, 16, pointer);
        Self { bytes, sync: true }
    }

    pub fn bytes(&self) -> &[u8; 24] {
        &self.bytes
    }

    pub fn is_sync(&self) -> bool {
        self.sync
    }

    pub fn set_kernel_result(&mut self, value: u32) {
        u32_at(&mut self.bytes, 8, value);
    }

    pub fn effective_result(&self, transport_result: i32) -> i32 {
        let result =
            i32::from_le_bytes([self.bytes[8], self.bytes[9], self.bytes[10], self.bytes[11]]);
        if !self.sync || transport_result < 0 || result == 0 {
            transport_result
        } else {
            result
        }
    }
}
