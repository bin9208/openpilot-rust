use openpilot_cluster::h264::vidc::{Config, Encoder, Packet};
use std::{error::Error, fmt::Write};

fn packet(packet: &Packet) -> Result<(), openpilot_encoderd::Error> {
    let mut hex = String::new();
    for byte in &packet.data {
        write!(hex, "{byte:02x}")
            .map_err(|error| openpilot_encoderd::Error::Io(std::io::Error::other(error)))?;
    }
    println!(
        "PACKET {} {} {} {} {hex}",
        packet.timestamp_us,
        packet.flags,
        u8::from(packet.codec_config),
        u8::from(packet.keyframe)
    );
    Ok(())
}
fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let [_, mode, width, height, rate, slice, priority] = args else {
        return Err("cluster_vidc_encoder_fixture MODE WIDTH HEIGHT RATE SLICE PRIORITY".into());
    };
    let mut encoder = Encoder::new(Config {
        width: width.parse()?,
        height: height.parse()?,
        fps: 10,
        bitrate: 2_000_000,
        gop: 1,
        rate_control: rate.parse()?,
        slice_max_bytes: slice.parse()?,
        realtime_priority: priority == "1",
        ..Config::default()
    })?;
    encoder.open()?;
    let layout = encoder.input_layout();
    println!(
        "OPEN {} {} {} {} {} {} {} {} {}",
        layout.width,
        layout.height,
        layout.stride,
        layout.y_scanlines,
        layout.uv_scanlines,
        layout.uv_offset,
        layout.byte_count,
        encoder.input_sizeimage(),
        encoder.capture_sizeimage()
    );
    let active =
        layout.uv_offset + usize::try_from(layout.stride)? * usize::try_from(layout.uv_scanlines)?;
    for frame in 0..12_u8 {
        let bytes = vec![frame; layout.byte_count];
        let timestamp = if frame == 11 {
            u64::MAX
        } else {
            u64::from(frame) * 100_000 + 99
        };
        if mode == "active" {
            encoder.encode_nv12_active(&bytes[..active], timestamp, &mut packet)?;
        } else if mode == "short" {
            encoder.encode_nv12(&bytes[..layout.byte_count - 1], timestamp, &mut packet)?;
        } else if mode == "acquire" || mode == "dmabuf" || mode == "cancel" {
            let mut index = encoder.acquire(&mut packet)?;
            if mode == "cancel" {
                encoder.cancel(index)?;
                assert!(encoder.cancel(index).is_err());
                assert!(encoder.with_input(index, |_, _| ()).is_err());
                index = encoder.acquire(&mut packet)?;
            }
            encoder.with_input(index, |bytes, fd| {
                assert!(rustix::fs::fstat(fd).is_ok());
                bytes.fill(frame);
            })?;
            encoder.submit(index, timestamp, mode == "dmabuf", &mut packet)?;
        } else {
            encoder.encode_nv12(&bytes, timestamp, &mut packet)?;
        }
        let timing = encoder.last_timings();
        assert_eq!(
            timing.total_us,
            [
                timing.pre_poll_us,
                timing.wait_input_us,
                timing.convert_us,
                timing.sync_us,
                timing.queue_us,
                timing.post_poll_us
            ]
            .into_iter()
            .fold(0, u64::wrapping_add)
        );
    }
    encoder.drain(0, &mut packet)?;
    encoder.close();
    encoder.close();
    assert!(!encoder.is_open());
    println!("CLOSED");
    Ok(())
}
fn main() {
    if let Err(error) = run(&std::env::args().collect::<Vec<_>>()) {
        let text = error.to_string();
        println!(
            "ERROR {}",
            text.strip_prefix("encoder source contract: ")
                .unwrap_or(&text)
        );
        println!("CLOSED");
    }
}
