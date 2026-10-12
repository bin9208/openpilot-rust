use openpilot_cluster::{
    h264::{Config, Control, Software},
    usb::{Display, Options, ParamsRuntime},
};
use openpilot_params::Params;
use serde::Deserialize;
use std::{io, path::PathBuf};

#[derive(Deserialize)]
struct Case {
    input: PathBuf,
    root: PathBuf,
    lock: PathBuf,
    dump: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    gop: u32,
    bitrate: String,
    chunk_size: i64,
    wait_ack: bool,
    soft_ack: bool,
    encoder: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, case_file] = args.as_slice() else {
        return Err("cluster_h264_fixture CASE".into());
    };
    let case: Case = serde_json::from_slice(&std::fs::read(case_file)?)?;
    let params = Params::open(&case.root, "fixture")?;
    let options = Options {
        display_fps: i32::try_from(case.fps)?,
        ..Options::default()
    };
    let config = Config {
        width: case.width,
        height: case.height,
        fps: case.fps,
        bitrate: case.bitrate,
        gop: case.gop,
        ffmpeg: "/usr/bin/ffmpeg".into(),
        encoder: case.encoder,
        requested_chunk_size: case.chunk_size,
        wait_for_ack: case.wait_ack,
        soft_ack: case.soft_ack,
        dump: Some(case.dump),
    };
    let bytes = std::fs::read(case.input)?;
    let width = case.width;
    let height = case.height;
    let size = usize::try_from(width)? * usize::try_from(height)? * 4;
    let mut pipeline = Software::start(config.clone(), move || {
        let mut display = Display::new(
            options,
            ParamsRuntime::at(&case.root, "fixture"),
            &case.lock,
        )
        .map_err(io::Error::other)?;
        display.set_orientation(2, false);
        display.open().map_err(io::Error::other)?;
        Ok(display)
    })?;
    let pid = pipeline.encoder_pid().ok_or("encoder pid missing")?;
    let command = std::fs::read(format!("/proc/{pid}/cmdline"))?;
    let initial = params.get_bool("ClusterHudConnected")?;
    pipeline.control(Control::Brightness(61))?;
    pipeline.control(Control::Orientation(1))?;
    let mut submitted = 0;
    for rgba in bytes.chunks_exact(size) {
        pipeline.submit_rgba(rgba, width, height)?;
        submitted += 1;
    }
    pipeline.close()?;
    let snapshot = pipeline.snapshot()?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "initial":initial,"closed":params.get_bool("ClusterHudConnected")?,
            "submitted":submitted,"bytes_sent":snapshot.bytes_sent,"chunks_sent":snapshot.chunks_sent,
            "stderr":snapshot.stderr_tail,"error":snapshot.error,
            "reaped":!std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "command":String::from_utf8(command)?.split('\0').filter(|item| !item.is_empty()).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
