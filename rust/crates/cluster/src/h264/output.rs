use crate::usb::{Display, H264Chunk, Runtime};
use std::io;

#[derive(Clone, Copy, Debug)]
pub enum Control {
    Brightness(i32),
    Orientation(i32),
    DisplayFps(i32),
}

/// Created and retained exclusively on the encoder-output worker.
pub trait Output {
    fn dimensions(&self) -> Option<[u32; 2]> {
        None
    }
    /// # Errors
    /// Returns stream initialization or chunk negotiation failure.
    fn start(&mut self, requested: i64) -> io::Result<usize>;
    /// # Errors
    /// Returns the existing transport's upload/acknowledgement failure.
    fn send(&mut self, chunk: H264Chunk<'_>) -> io::Result<()>;
    /// # Errors
    /// Returns a control dispatch failure.
    fn control(&mut self, control: Control) -> io::Result<()>;
    fn stop(&mut self);
    fn drain_profile_samples(&mut self) -> Vec<(String, f64)> {
        Vec::new()
    }
}

impl<R: Runtime> Output for Display<R> {
    fn dimensions(&self) -> Option<[u32; 2]> {
        Some(Display::dimensions(self))
    }
    fn start(&mut self, requested: i64) -> io::Result<usize> {
        usize::try_from(
            self.start_h264_stream(requested)
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)
    }
    fn send(&mut self, chunk: H264Chunk<'_>) -> io::Result<()> {
        self.send_h264_chunk(chunk).map_err(io::Error::other)
    }
    fn control(&mut self, control: Control) -> io::Result<()> {
        match control {
            Control::Brightness(value) => {
                self.set_brightness(value, false);
            }
            Control::Orientation(value) => {
                self.set_orientation(value, false);
            }
            Control::DisplayFps(value) => {
                self.set_display_fps(value, false);
            }
        }
        Ok(())
    }
    fn drain_profile_samples(&mut self) -> Vec<(String, f64)> {
        let samples = self
            .profile_samples()
            .iter()
            .map(|&(name, ms)| (name.into(), ms))
            .collect();
        self.clear_profile_samples();
        samples
    }
    fn stop(&mut self) {
        self.stop_h264_stream();
    }
}
