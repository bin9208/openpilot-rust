use openpilot_cluster::{
    h264::Control,
    jpeg_async::{
        codec::{Codec, Config},
        DisplayOutput, Frame, Pipeline,
    },
    runtime_args::JpegEncoder,
    usb::{Display, Options, Runtime},
    Error,
};
use std::{
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
#[derive(serde::Deserialize)]
struct Query {
    lock: PathBuf,
    pixels: PathBuf,
    width: u32,
    height: u32,
}
struct Clock;
impl Runtime for Clock {
    fn milliseconds(&self) -> Result<u32, Error> {
        Ok(43_210)
    }
    fn sleep(&mut self, _duration: Duration) {}
    fn gpu_active(&self) -> bool {
        false
    }
    fn reset_protected(&self) -> bool {
        false
    }
    fn connected(&mut self, _value: bool) {}
    fn warning(&mut self, message: &str) {
        eprintln!("{message}");
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let q: Query = serde_json::from_str(&text)?;
    let mut pipeline = Pipeline::start(move || {
        let mut display =
            Display::new(Options::default(), Clock, &q.lock).map_err(io::Error::other)?;
        display.set_profile_enabled(true);
        display.open().map_err(io::Error::other)?;
        Ok(DisplayOutput::new(
            display,
            Codec::new(Config {
                quality: 82,
                encoder: JpegEncoder::Pillow,
                turbo_library: PathBuf::from("unused"),
                profile_enabled: true,
            }),
        ))
    })?;
    let dimensions = pipeline.dimensions();
    pipeline.control(Control::Brightness(77))?;
    pipeline.control(Control::Orientation(2))?;
    pipeline.control(Control::DisplayFps(10))?;
    pipeline.submit_rgba(Frame {
        rgba: Arc::from(std::fs::read(q.pixels)?),
        width: q.width,
        height: q.height,
    })?;
    assert!(pipeline.close()?);
    pipeline.check_error()?;
    let stages: Vec<_> = pipeline
        .profile_samples()?
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    println!(
        "{}",
        serde_json::json!({"dimensions":dimensions,"closed":true,"stages":stages})
    );
    Ok(())
}
