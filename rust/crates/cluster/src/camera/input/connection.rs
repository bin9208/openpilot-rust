use super::{
    copy_frame, Error, Input, VisionClient, CONNECTION_RETRY_SECONDS, FRAME_POLL_SECONDS,
    FRAME_STALE_SECONDS,
};
use std::time::Duration;

impl Input {
    /// # Errors
    /// Returns a native connection/receive failure; absent or stale input has no frame.
    pub fn poll(&mut self, now: f64) -> Result<(), Error> {
        if !self.ensure_connection(now)? || now - self.last_poll < FRAME_POLL_SECONDS {
            return Ok(());
        }
        self.last_poll = now;
        let client = self
            .client
            .as_mut()
            .ok_or(Error::Contract("camera client is closed"))?;
        if let Some(frame) = client.receive(Duration::ZERO)? {
            self.frame = Some(copy_frame(&frame, self.frame.take(), self.copy)?);
            self.last_frame_at = now;
            self.revision = self.revision.wrapping_add(1);
        } else if !client.is_connected() {
            self.reset()?;
        } else if self.connected_at > 0.0
            && now - self.connected_at.max(self.last_frame_at) > FRAME_STALE_SECONDS
        {
            println!("Cluster road camera stream stale; reconnecting VisionIPC");
            self.reset()?;
        }
        Ok(())
    }
    fn ensure_connection(&mut self, now: f64) -> Result<bool, Error> {
        let client = self
            .client
            .as_mut()
            .ok_or(Error::Contract("camera client is closed"))?;
        if client.is_connected() {
            if self.connected_at <= 0.0 {
                self.connected_at = now;
            }
            return Ok(true);
        }
        self.frame = None;
        if now - self.last_attempt < CONNECTION_RETRY_SECONDS {
            return Ok(false);
        }
        self.last_attempt = now;
        if client.connect()? && client.layout().is_some() {
            self.connected_at = now;
            let layout = client
                .layout()
                .ok_or(Error::Contract("connected camera has no buffers"))?;
            println!(
                "Cluster road camera connected: {}x{} NV12 GPU upload",
                layout.width, layout.height
            );
            self.waiting_logged = false;
            self.discover(now);
            Ok(true)
        } else {
            if !self.waiting_logged {
                println!("Cluster road camera waiting for local camerad VisionIPC");
                self.waiting_logged = true;
            }
            Ok(false)
        }
    }
    fn reset(&mut self) -> Result<(), Error> {
        self.frame = None;
        self.connected_at = 0.0;
        self.last_frame_at = 0.0;
        self.client = Some(VisionClient::new(&self.name, self.stream, true)?);
        self.target = None;
        self.waiting_logged = false;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}
