use openpilot_cluster::media::{
    vidc::{Config, Decoder},
    HardwareFrame,
};
use std::{
    io,
    os::{fd::AsFd, unix::fs::FileExt},
    sync::Arc,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, scenario] = args.as_slice() else {
        return Err("cluster_vidc_fixture CASE".into());
    };
    let mut decoder = Decoder::new(Config {
        width: 96,
        height: 64,
        ..Config::default()
    })?;
    let mut retained: Vec<Arc<HardwareFrame>> = Vec::new();
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        decoder.open()?;
        decoder.open()?;
        println!("OPEN");
        for (step, sequence) in [0, 1, 1_234_567, u64::MAX, 99].into_iter().enumerate() {
            let mut packet = [0_u8; 16];
            for (index, byte) in packet.iter_mut().enumerate() {
                *byte = u8::try_from(index + step * 7)?;
            }
            let Some(frame) = decoder.decode(&packet, sequence)? else {
                println!("NONE");
                continue;
            };
            let file = std::fs::File::from(frame.as_fd().try_clone_to_owned()?);
            let mut pixels = [0_u8; 64];
            if file.read_at(&mut pixels, 0)? != pixels.len() {
                return Err(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "capture bytes absent").into(),
                );
            }
            let mut bytes = String::with_capacity(128);
            for byte in pixels {
                use std::fmt::Write;
                write!(&mut bytes, "{byte:02x}")?;
            }
            println!(
                "FRAME {} {} {} {} {} {} {bytes}",
                frame.index,
                frame.layout.width,
                frame.layout.height,
                frame.layout.stride,
                frame.layout.uv_offset,
                frame.sequence()
            );
            if scenario == "leased-port-change" {
                retained.push(frame);
            } else {
                drop(frame);
                decoder.drain_releases()?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        let message = error.to_string();
        println!(
            "ERROR {}",
            message
                .strip_prefix("encoder source contract: ")
                .unwrap_or(&message)
        );
    }
    decoder.close();
    decoder.close();
    drop(retained);
    println!("CLOSED");
    Ok(())
}
