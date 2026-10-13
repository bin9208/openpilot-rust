use super::{
    copy_frame, Error, Input, Target, VisionClient, VisionStream, STREAM_DISCOVERY_SECONDS,
};
use std::time::Duration;

impl Input {
    /// # Errors
    /// Returns a target queue/connection/receive failure. A switch waits for its first frame.
    pub fn select_stream(&mut self, prefer_wide: bool, now: f64) -> Result<bool, Error> {
        self.discover(now);
        let wide = self.available.contains(&VisionStream::WideRoad);
        let desired = if wide && (prefer_wide || !self.available.contains(&VisionStream::Road)) {
            VisionStream::WideRoad
        } else {
            VisionStream::Road
        };
        if desired == self.stream {
            self.target = None;
            return Ok(self.is_wide());
        }
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.stream != desired)
        {
            self.target = Some(Target {
                stream: desired,
                client: VisionClient::new(&self.name, desired, true)?,
            });
        }
        let target = self
            .target
            .as_mut()
            .ok_or(Error::Contract("camera target missing"))?;
        if !target.client.is_connected()
            && (!target.client.connect()? || target.client.layout().is_none())
        {
            return Ok(self.is_wide());
        }
        let Some(frame) = target.client.receive(Duration::ZERO)? else {
            return Ok(self.is_wide());
        };
        self.frame = Some(copy_frame(&frame, self.frame.take(), self.copy)?);
        let target = self
            .target
            .take()
            .ok_or(Error::Contract("camera target missing"))?;
        self.stream = desired;
        self.client = Some(target.client);
        self.connected_at = now;
        self.last_frame_at = now;
        self.revision = self.revision.wrapping_add(1);
        self.generation = self.generation.wrapping_add(1);
        let frame = self
            .frame
            .as_ref()
            .ok_or(Error::Contract("camera frame missing"))?;
        println!(
            "Cluster camera switched to {} stream: {}x{}",
            if self.is_wide() { "wide" } else { "road" },
            frame.metadata.width,
            frame.metadata.height
        );
        Ok(self.is_wide())
    }
    pub(super) fn discover(&mut self, now: f64) {
        if now - self.last_discovery < STREAM_DISCOVERY_SECONDS {
            return;
        }
        self.last_discovery = now;
        // Source discovery catches provider errors and retains previous nonempty discovery.
        if let Ok(streams) = VisionClient::available_streams(&self.name) {
            if !streams.is_empty() {
                self.available = streams;
            }
        }
    }
}
