use openpilot_cluster::{
    usb::{Display, H264Chunk, Options, ParamsRuntime},
    Error,
};
use openpilot_params::Params;
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(serde::Deserialize)]
struct Case {
    root: PathBuf,
    lock: PathBuf,
    stream: bool,
    active: bool,
}

fn main() -> Result<(), Error> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let case: Case = serde_json::from_str(&input)?;
    let params = Params::open(&case.root, "fixture")?;
    let options = Options {
        display_fps: if case.active { 5 } else { 10 },
        ..Options::default()
    };
    let mut display = Display::new(
        options,
        ParamsRuntime::at(&case.root, "fixture"),
        &case.lock,
    )?;
    display.open()?;
    let initial = params.get_bool("ClusterHudConnected")?;
    let jpeg = display
        .send_jpeg(&[1, 2, 3])
        .map_err(|error| error.to_string());
    let after_frame = params.get_bool("ClusterHudConnected")?;
    let chunk_size = if case.stream {
        let size = display.start_h264_stream(0)?;
        display.send_h264_chunk(H264Chunk {
            bytes: &[0, 0, 1, 65, 12],
            last: false,
            wait_for_ack: false,
            require_ack: false,
        })?;
        display.stop_h264_stream();
        Some(size)
    } else {
        None
    };
    display.close();
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "initial":initial,"jpeg":jpeg,"after_frame":after_frame,"chunk_size":chunk_size,
            "closed":params.get_bool("ClusterHudConnected")?,
        }))?
    );
    Ok(())
}
