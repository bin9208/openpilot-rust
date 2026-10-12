use super::{codec::Codec, Frame, Output, Samples};
use crate::{
    h264::Control,
    usb::{Display, Runtime},
};
use std::io;
pub struct DisplayOutput<R: Runtime> {
    display: Display<R>,
    codec: Codec,
}
impl<R: Runtime> DisplayOutput<R> {
    #[must_use]
    pub const fn new(display: Display<R>, codec: Codec) -> Self {
        Self { display, codec }
    }
}
impl<R: Runtime> Output for DisplayOutput<R> {
    fn dimensions(&self) -> Option<[u32; 2]> {
        Some(self.display.dimensions())
    }
    fn clear_profile_samples(&mut self) {
        self.codec.clear_profile_samples();
        self.display.clear_profile_samples();
    }
    fn encode_jpeg(&mut self, frame: &Frame) -> io::Result<Vec<u8>> {
        self.codec.encode(frame)
    }
    fn send_jpeg(&mut self, jpeg: &[u8]) -> io::Result<()> {
        self.display.send_jpeg(jpeg).map_err(io::Error::other)
    }
    fn profile_samples(&self) -> Samples {
        let mut samples = self.codec.profile_samples().to_vec();
        samples.extend(
            self.display
                .profile_samples()
                .iter()
                .map(|&(name, time)| (name.into(), time)),
        );
        samples
    }
    fn control(&mut self, control: Control) -> io::Result<()> {
        match control {
            Control::Brightness(v) => {
                self.display.set_brightness(v, false);
            }
            Control::Orientation(v) => {
                self.display.set_orientation(v, false);
            }
            Control::DisplayFps(v) => {
                self.display.set_display_fps(v, false);
            }
        }
        Ok(())
    }
}
