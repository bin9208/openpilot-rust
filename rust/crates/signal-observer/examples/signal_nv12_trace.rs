use serde::Deserialize;
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Input {
    path: PathBuf,
    width: u32,
    height: u32,
    stride: usize,
    uv_offset: usize,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let mut out = io::stdout().lock();
    for i in cases {
        let rgb = openpilot_signal_observer::worker::copy_nv12_rgb(
            &fs::read(i.path)?,
            i.width,
            i.height,
            i.stride,
            i.uv_offset,
        )?;
        out.write_all(&rgb.data)?;
    }
    Ok(())
}
