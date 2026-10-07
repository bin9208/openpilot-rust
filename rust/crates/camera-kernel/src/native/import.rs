use super::{Buffer, CallResult, Device, Error, MmuHandles};
use std::os::fd::{AsRawFd, BorrowedFd};

#[derive(Debug)]
pub struct ImportedBuffers {
    pub raw: u32,
    pub yuv: u32,
    pub yuv_result: Option<CallResult>,
}

impl Device {
    pub fn import_images(
        &self,
        mmu: MmuHandles,
        bps: bool,
        raw: Option<BorrowedFd<'_>>,
        yuv: Option<BorrowedFd<'_>>,
    ) -> Result<ImportedBuffers, Error> {
        let mut data = Buffer::<96>::zeroed();
        data.put32(0, u32::from_le_bytes(mmu.device.to_le_bytes()));
        data.put32(64, if bps { 2 } else { 1 });
        data.put32(68, 1);
        if bps {
            data.put32(4, u32::from_le_bytes(mmu.icp.to_le_bytes()));
        }
        let mut raw_handle = 0;
        if let Some(fd) = raw {
            data.put32(72, u32::from_le_bytes(fd.as_raw_fd().to_le_bytes()));
            // SAFETY: FFI: MAP_BUF consumes this 96-byte ABI; the borrowed image FD stays open.
            unsafe { self.camera(0x113, &mut data) }.require_success(0x113)?;
            raw_handle = data.get32(80);
            super::diagnostics::message(
                true,
                format!("map buf req: (fd: {}) 0x{raw_handle:x} 0", fd.as_raw_fd()),
            );
        }
        let (yuv_handle, yuv_result) = match yuv {
            Some(fd) => {
                data.put32(72, u32::from_le_bytes(fd.as_raw_fd().to_le_bytes()));
                // SAFETY: FFI: source deliberately reuses the map output and keeps the image FD borrowed.
                let result = unsafe { self.camera(0x113, &mut data) };
                super::diagnostics::message(
                    true,
                    format!(
                        "map buf req: (fd: {}) 0x{:x} {}",
                        fd.as_raw_fd(),
                        data.get32(80),
                        result.code
                    ),
                );
                (data.get32(80), Some(result))
            }
            None => (0, None),
        };
        Ok(ImportedBuffers {
            raw: raw_handle,
            yuv: yuv_handle,
            yuv_result,
        })
    }
}
