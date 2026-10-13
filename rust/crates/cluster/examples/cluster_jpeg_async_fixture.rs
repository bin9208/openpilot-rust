use openpilot_cluster::{
    jpeg_async::{
        codec::{Codec, Config},
        Frame, Output, Pipeline, Samples,
    },
    runtime_args::JpegEncoder,
};
use serde::Deserialize;
use std::{
    io::{self, BufRead, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::Arc,
};
#[derive(Deserialize)]
struct Query {
    mode: String,
    encoder: String,
    quality: i64,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    pixels: PathBuf,
    #[serde(default)]
    output: PathBuf,
    turbo_library: PathBuf,
    #[serde(default)]
    profile: bool,
    #[serde(default)]
    socket: PathBuf,
}
fn config(q: &Query) -> io::Result<Config> {
    Ok(Config {
        quality: q.quality,
        encoder: match q.encoder.as_str() {
            "auto" => JpegEncoder::Auto,
            "pillow" => JpegEncoder::Pillow,
            "turbojpeg" => JpegEncoder::Turbojpeg,
            _ => return Err(io::Error::other("unknown encoder")),
        },
        turbo_library: q.turbo_library.clone(),
        profile_enabled: q.profile,
    })
}
struct Recipient {
    codec: Codec,
    socket: UnixStream,
}
impl Output for Recipient {
    fn clear_profile_samples(&mut self) {
        self.codec.clear_profile_samples();
    }
    fn encode_jpeg(&mut self, frame: &Frame) -> io::Result<Vec<u8>> {
        self.codec.encode(frame)
    }
    fn send_jpeg(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.socket.write_all(
            &u64::try_from(bytes.len())
                .map_err(io::Error::other)?
                .to_le_bytes(),
        )?;
        self.socket.write_all(bytes)?;
        let mut ack = [0];
        self.socket.read_exact(&mut ack)?;
        if ack[0] == 0 {
            Err(io::Error::other("owned recipient rejected JPEG"))
        } else {
            Ok(())
        }
    }
    fn profile_samples(&self) -> Samples {
        self.codec.profile_samples().to_vec()
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Command {
    Submit {
        pixels: PathBuf,
        width: u32,
        height: u32,
    },
    Capacity {
        timeout: f64,
    },
    Samples,
    Check,
    Close,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lines = std::io::stdin().lock().lines();
    let q: Query = serde_json::from_str(&lines.next().ok_or("missing query")??)?;
    let conf = config(&q)?;
    if q.mode == "codec" {
        let mut codec = Codec::new(conf);
        let frame = Frame {
            rgba: Arc::from(std::fs::read(q.pixels)?),
            width: q.width,
            height: q.height,
        };
        let (bytes, error) = match codec.encode_pixels(&frame.rgba, frame.width, frame.height) {
            Ok(jpeg) => {
                std::fs::write(q.output, &jpeg)?;
                (Some(jpeg.len()), None)
            }
            Err(error) => (None, Some(error.to_string())),
        };
        println!(
            "{}",
            serde_json::to_string(
                &serde_json::json!({"bytes":bytes,"error":error,"stages":codec.profile_samples().iter().map(|(n,_)|n).collect::<Vec<_>>()})
            )?
        );
        return Ok(());
    }
    let socket = q.socket;
    let mut pipeline = Pipeline::start(move || {
        Ok(Recipient {
            codec: Codec::new(conf),
            socket: UnixStream::connect(socket)?,
        })
    })?;
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"started":true,"dimensions":pipeline.dimensions()})
        )?
    );
    std::io::stdout().flush()?;
    for line in lines {
        let command: Command = serde_json::from_str(&line?)?;
        let result: Result<serde_json::Value, Box<dyn std::error::Error>> = match command {
            Command::Submit {
                pixels,
                width,
                height,
            } => pipeline
                .submit_rgba(Frame {
                    rgba: Arc::from(std::fs::read(pixels)?),
                    width,
                    height,
                })
                .map(|()| serde_json::Value::Null)
                .map_err(Into::into),
            Command::Capacity { timeout } => pipeline
                .wait_for_capacity(Some(std::time::Duration::try_from_secs_f64(
                    timeout.max(0.0),
                )?))
                .map(serde_json::Value::Bool)
                .map_err(Into::into),
            Command::Samples => pipeline
                .profile_samples()
                .map(|v| serde_json::json!(v))
                .map_err(Into::into),
            Command::Check => pipeline
                .check_error()
                .map(|()| serde_json::Value::Null)
                .map_err(Into::into),
            Command::Close => pipeline
                .close()
                .map(serde_json::Value::Bool)
                .map_err(Into::into),
        };
        let value = match result {
            Ok(value) => serde_json::json!({"ok":value}),
            Err(error) => {
                serde_json::json!({"error":error.to_string(),"cause":error.source().map(ToString::to_string)})
            }
        };
        println!("{}", serde_json::to_string(&value)?);
        std::io::stdout().flush()?;
    }
    let _ = pipeline.close()?;
    Ok(())
}
