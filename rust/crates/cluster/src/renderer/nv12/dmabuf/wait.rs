use super::{DmaTargetPool, Error, Result};

impl DmaTargetPool {
    /// # Errors
    /// Returns missing fence creation, a fifty-millisecond timeout or a GPU wait failure.
    pub fn wait_for_gpu(&mut self) -> Result<()> {
        self.clear_errors("encoder DMA-BUF fence")?;
        let fence = self.gl.fence(0x9117, 0)?;
        if fence == 0 {
            return Err(Error::FenceCreation);
        }
        let result = (|| -> Result<()> {
            let status = self.gl.wait_fence(fence, 1, 50_000_000)?;
            if matches!(status, 0x911A | 0x911C) {
                return Ok(());
            }
            let error = self.gl.error();
            match status {
                0x911B => Err(Error::Timeout),
                0x911D => Err(Error::Fence(error)),
                status => Err(Error::FenceStatus(status)),
            }
        })();
        self.gl.delete_fence(fence)?;
        result
    }
}
