#![allow(unsafe_code)]
use openpilot_cereal::log_capnp::{encode_data, event};
use openpilot_encoderd::{
    config::Mode,
    native::{publisher::VideoPublisher, v4l::V4l, Mapping},
    profile::{cameras, Recording},
};
use openpilot_msgq::{Subscriber, VisionMetadata};
use std::{
    fs::{self, File},
    io::Write,
    os::fd::OwnedFd,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

fn data(event: event::Reader<'_>) -> Result<encode_data::Reader<'_>, Box<dyn std::error::Error>> {
    use event::Which;
    Ok(match event.which()? {
        Which::RoadEncodeData(value)
        | Which::DriverEncodeData(value)
        | Which::WideRoadEncodeData(value)
        | Which::QRoadEncodeData(value)
        | Which::LivestreamRoadEncodeData(value)
        | Which::LivestreamDriverEncodeData(value)
        | Which::LivestreamWideRoadEncodeData(value)
        | Which::YoutubeRoadEncodeData(value) => value?,
        _ => return Err("unexpected encoder service".into()),
    })
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|value| format!("{value:02x}")).collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.len() != 8 {
        return Err("usage: v4l_trace output mode camera encoder width height frames".into());
    }
    let directory = PathBuf::from(&arguments[1]);
    fs::create_dir_all(&directory)?;
    let mode =
        Mode::parse((arguments[2] != "main").then_some(arguments[2].as_str())).ok_or("mode")?;
    let camera_index: usize = arguments[3].parse()?;
    let encoder_index: usize = arguments[4].parse()?;
    let width: usize = arguments[5].parse()?;
    let height: usize = arguments[6].parse()?;
    let frames: u32 = arguments[7].parse()?;
    let profiles = cameras(
        mode,
        Recording {
            road: true,
            wide: true,
            front: true,
            audio: true,
        },
    );
    let info = &profiles[camera_index].encoders[encoder_index];
    let input = (width.try_into()?, height.try_into()?);
    let settings = info.quality.settings(input.0, false, 600_000);
    let output = if info.publish == "livestreamRoadEncodeData" {
        (964, 604)
    } else {
        info.dimensions(input.0, input.1, settings)
    };
    let publisher = VideoPublisher::new(info.publish, output, settings.codec)?;
    let capacity = openpilot_messaging::services::SERVICES
        .iter()
        .find(|entry| entry.name == info.publish)
        .ok_or("service")?
        .queue_size;
    let mut subscriber = Subscriber::for_runtime(info.publish, false, capacity)?;
    let mut encoder = V4l::new(info, input, output, settings, publisher)?;
    let mut trace = File::create(directory.join("trace.tsv"))?;
    for segment in 0..2 {
        for frame in 0..frames {
            let frame_id = 100 + segment * frames + frame;
            let length = width * height * 3 / 2;
            let path = directory.join(format!("input-{frame_id}.nv12"));
            let mut backing = File::options()
                .write(true)
                .read(true)
                .create_new(true)
                .open(&path)?;
            let pixels: Vec<_> = (0..length)
                .map(|index| ((index * 17 + frame_id as usize * 7) % 256) as u8)
                .collect();
            backing.write_all(&pixels)?;
            let fd: OwnedFd = backing.into();
            // SAFETY: [bounds/lifetime] this file owns length initialized bytes;
            // the mapping's FD pins storage after unlink and asynchronous submit.
            let mapping = Arc::new(unsafe { Mapping::new(fd, length)? });
            fs::remove_file(path)?;
            let metadata = VisionMetadata {
                width,
                height,
                stride: width,
                uv_offset: width * height,
                len: length,
                frame_id,
                timestamp_sof: 1_000_000_000 + u64::from(frame_id) * 50_000_000,
                timestamp_eof: 1_020_000_000 + u64::from(frame_id) * 50_000_000,
                valid: true,
                received: true,
                index: 0,
                fd: -1,
            };
            if encoder.encode(mapping, length, &metadata)? != i32::try_from(frame)? {
                return Err("V4L frame counter".into());
            }
        }
        encoder.idle(true);
        if segment == 0 {
            encoder.rotate()?;
        } else {
            encoder.close()?;
        }
        for _ in 0..frames {
            let bytes = subscriber
                .receive(Duration::from_secs(2))?
                .ok_or("missing V4L publication")?;
            let message = capnp::serialize::read_message(
                bytes.as_slice(),
                capnp::message::ReaderOptions::new(),
            )?;
            let event = message.get_root::<event::Reader<'_>>()?;
            if !event.get_valid() || event.get_log_mono_time() == 0 {
                return Err("invalid event envelope".into());
            }
            let packet = data(event)?;
            if packet.get_unix_timestamp_nanos() == 0 {
                return Err("missing wall clock".into());
            }
            let index = packet.get_idx()?;
            let bytes = packet.get_data()?;
            if usize::try_from(index.get_len())? != bytes.len() {
                return Err("packet length metadata".into());
            }
            writeln!(
                trace,
                "P\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                index.get_segment_num(),
                index.get_segment_id(),
                index.get_frame_id(),
                index.get_timestamp_sof(),
                index.get_timestamp_eof(),
                index.get_flags(),
                hex(packet.get_header()?),
                hex(bytes),
                packet.get_width(),
                packet.get_height(),
                index.get_encode_id(),
                index.get_type()? as u16
            )?;
        }
        encoder.idle(false);
    }
    if subscriber.receive(Duration::ZERO)?.is_some() {
        return Err("unexpected EOS/config publication".into());
    }
    drop(encoder);
    Ok(())
}
