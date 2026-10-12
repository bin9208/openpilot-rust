use std::{io, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: String,
    pub gop: u32,
    pub ffmpeg: PathBuf,
    pub encoder: String,
    pub requested_chunk_size: i64,
    pub wait_for_ack: bool,
    pub soft_ack: bool,
    pub dump: Option<PathBuf>,
}

impl Config {
    pub(super) fn bytes(&self) -> io::Result<usize> {
        usize::try_from(self.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(self.height).ok()?))
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes > 0)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid H264 RGBA dimensions")
            })
    }

    pub(super) fn resolve(&self) -> io::Result<(String, String)> {
        let encoder = if self.encoder == "auto" {
            let names = super::probe::names(&self.ffmpeg, "-encoders", true).unwrap_or_default();
            ["h264_v4l2m2m", "h264_omx", "libx264"]
                .into_iter()
                .find(|name| names.contains(*name))
                .unwrap_or("libx264")
                .to_owned()
        } else {
            self.encoder.clone()
        };
        let names = super::probe::names(&self.ffmpeg, "-muxers", false)?;
        let muxer = ["h264", "rawvideo"]
            .into_iter()
            .find(|name| names.contains(*name))
            .ok_or_else(|| {
                io::Error::other("ffmpeg does not provide a usable raw H264 stdout muxer")
            })?;
        Ok((encoder, muxer.into()))
    }

    #[must_use]
    pub fn arguments(&self, encoder: &str, muxer: &str) -> Vec<String> {
        let fps = self.fps.max(1).to_string();
        let gop = self.gop.max(1).to_string();
        let dimensions = format!("{}x{}", self.width, self.height);
        let mut args: Vec<String> = [
            "-hide_banner",
            "-loglevel",
            "warning",
            "-fflags",
            "nobuffer",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s:v",
            &dimensions,
            "-framerate",
            &fps,
            "-i",
            "pipe:0",
            "-an",
            "-c:v",
            encoder,
            "-b:v",
            &self.bitrate,
            "-maxrate",
            &self.bitrate,
            "-bufsize",
            &self.bitrate,
            "-g",
            &gop,
            "-bf",
            "0",
            "-flags",
            "+low_delay",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        if encoder == "libx264" {
            args.extend(
                [
                    "-preset",
                    "ultrafast",
                    "-tune",
                    "zerolatency",
                    "-profile:v",
                    "baseline",
                    "-pix_fmt",
                    "yuv420p",
                    "-x264-params",
                ]
                .into_iter()
                .map(str::to_owned),
            );
            args.push(format!(
                "keyint={gop}:min-keyint={gop}:scenecut=0:repeat-headers=1"
            ));
        } else {
            args.extend(
                [
                    "-pix_fmt",
                    if encoder == "h264_v4l2m2m" {
                        "nv12"
                    } else {
                        "yuv420p"
                    },
                ]
                .into_iter()
                .map(str::to_owned),
            );
        }
        args.extend(
            ["-flush_packets", "1", "-f", muxer, "pipe:1"]
                .into_iter()
                .map(str::to_owned),
        );
        args
    }
}
