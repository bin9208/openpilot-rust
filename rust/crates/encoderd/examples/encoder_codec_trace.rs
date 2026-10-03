#![allow(unsafe_code)]
use openpilot_encoderd::{
    config::Codec,
    native::{software::Software, thumbnail::Thumbnail, Mapping},
};
use openpilot_msgq::VisionMetadata;
use std::{
    fs::{self, File},
    io::{Seek, SeekFrom, Write},
    os::fd::OwnedFd,
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.len() != 9 {
        return Err(
            "usage: trace output codec width height out_width out_height stride frames".into(),
        );
    }
    let directory = PathBuf::from(&arguments[1]);
    fs::create_dir_all(&directory)?;
    let width: usize = arguments[3].parse()?;
    let height: usize = arguments[4].parse()?;
    let output_width: usize = arguments[5].parse()?;
    let output_height: usize = arguments[6].parse()?;
    let stride: usize = arguments[7].parse()?;
    let frames: u32 = arguments[8].parse()?;
    let uv_offset = stride * height + stride * 4;
    let length = uv_offset + stride * height / 2;
    let mut backing = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(directory.join("input.nv12"))?;
    backing.set_len(length.try_into()?)?;
    let owned: OwnedFd = backing.try_clone()?.into();
    // SAFETY: [bounds/lifetime] the retained file is exactly length bytes and
    // never shrunk; synchronous codec calls finish before the next file write.
    let mapping = unsafe { Mapping::new(owned, length)? };
    let mut trace = File::create(directory.join("trace.tsv"))?;
    let mut packet_number = 0;
    let mut pixels = vec![0; length];
    let mut jpeg = if arguments[2] == "jpeg" {
        Some(Thumbnail::new(output_width, output_height)?)
    } else {
        None
    };
    let mut codec = if jpeg.is_none() {
        Some(Software::new(
            (width.try_into()?, height.try_into()?),
            (output_width.try_into()?, output_height.try_into()?),
            "fixture",
        )?)
    } else {
        None
    };
    let kind = if arguments[2] == "h264" {
        Codec::QcameraH264
    } else {
        Codec::BigBoxLossless
    };
    for segment in 0..2 {
        if let Some(codec) = &mut codec {
            codec.open(kind, 20)?;
        }
        for offset in 0..frames {
            let frame_id = 100 + segment * frames + offset;
            for (index, byte) in pixels.iter_mut().enumerate() {
                *byte = ((index * 17 + (index / stride) * 13 + frame_id as usize * 7) % 256) as u8;
            }
            backing.seek(SeekFrom::Start(0))?;
            backing.write_all(&pixels)?;
            let metadata = VisionMetadata {
                width,
                height,
                stride,
                uv_offset,
                len: length,
                frame_id,
                timestamp_sof: 1_000_000_000 + u64::from(frame_id) * 50_000_000,
                timestamp_eof: 1_020_000_000 + u64::from(frame_id) * 50_000_000,
                valid: true,
                received: true,
                index: 0,
                fd: -1,
            };
            if let Some(codec) = &mut codec {
                let result = codec.encode(&mapping, &metadata, |segment, index, flags, data| {
                    fs::write(directory.join(format!("packet-{packet_number}.bin")), data)?;
                    writeln!(trace, "P\t{packet_number}\t{segment}\t{index}\t{frame_id}\t{}\t{}\t{flags}\t{}", metadata.timestamp_sof, metadata.timestamp_eof, data.len())?;
                    packet_number += 1;
                    Ok(())
                }, |operation, code| eprintln!("{operation} {code}"))?;
                writeln!(trace, "R\t{frame_id}\t{result}")?;
            }
            if let Some(jpeg) = &mut jpeg {
                let bytes = jpeg.generate(&mapping, &metadata)?;
                fs::write(
                    directory.join(format!("packet-{packet_number}.bin")),
                    &bytes,
                )?;
                writeln!(
                    trace,
                    "J\t{packet_number}\t{frame_id}\t{}\t{}",
                    metadata.timestamp_eof,
                    bytes.len()
                )?;
                packet_number += 1;
            }
        }
        if let Some(codec) = &mut codec {
            codec.close();
        }
    }
    drop(mapping);
    drop(backing);
    fs::remove_file(directory.join("input.nv12"))?;
    Ok(())
}
