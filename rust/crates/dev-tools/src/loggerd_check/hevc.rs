//! Real ffmpeg HEVC, including zero dimensions and separate codec header.
use super::{require, save, Result};
use crate::qa_process::{CommandSpec, NativeRunner};
use openpilot_cereal::log_capnp::{encode_index, event};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
pub(super) struct Sample {
    pub raw: Vec<u8>,
    pub records: Vec<Vec<u8>>,
}
pub(super) fn fixture(output: &Path, cancelled: &AtomicBool) -> Result<Sample> {
    fs::create_dir_all(output.parent().ok_or("HEVC fixture parent absent")?)?;
    fs::create_dir(output)?;
    let path = output.join("native.hevc");
    let path_text = path.to_str().ok_or("HEVC path not UTF-8")?;
    let argv = [
        "ffmpeg",
        "-nostdin",
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=160x120:rate=20",
        "-frames:v",
        "8",
        "-c:v",
        "libx265",
        "-x265-params",
        "log-level=error:pools=1:frame-threads=1",
        "-f",
        "hevc",
        path_text,
    ]
    .map(str::to_owned)
    .to_vec();
    let result = NativeRunner.run_controlled(
        &CommandSpec::new(argv.clone(), Duration::from_secs(60)),
        Instant::now() + Duration::from_secs(60),
        cancelled,
    )?;
    fs::write(
        output.join("encoder.log"),
        [result.stdout, result.stderr].concat(),
    )?;
    require(result.code == 0, "actual HEVC encoder failed")?;
    let probe = NativeRunner.run_controlled(
        &CommandSpec::new(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_packets",
                "-of",
                "json",
                path_text,
            ]
            .map(str::to_owned)
            .to_vec(),
            Duration::from_secs(30),
        ),
        Instant::now() + Duration::from_secs(30),
        cancelled,
    )?;
    require(probe.code == 0, "actual HEVC packet probe failed")?;
    fs::write(output.join("packets.json"), &probe.stdout)?;
    let packets: Value = serde_json::from_slice(&probe.stdout)?;
    let raw = fs::read(&path)?;
    let mut reconstructed = Vec::new();
    let mut records = Vec::new();
    fn number(value: &Value) -> Result<usize> {
        Ok(usize::try_from(
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
                .ok_or("HEVC packet offset/size missing")?,
        )?)
    }
    let delimiters = regex::bytes::Regex::new(r"\x00\x00(?:\x00)?\x01")?;
    for (sequence, packet) in packets["packets"]
        .as_array()
        .ok_or("HEVC packet list missing")?
        .iter()
        .enumerate()
    {
        let start = number(&packet["pos"])?;
        let size = number(&packet["size"])?;
        let end = start
            .checked_add(size)
            .ok_or("HEVC packet range overflow")?;
        let mut data = raw
            .get(start..end)
            .ok_or("HEVC packet outside actual bitstream")?;
        reconstructed.extend_from_slice(data);
        let mut header = &[][..];
        if sequence == 0 {
            for marker in delimiters.find_iter(data) {
                let byte = *data.get(marker.end()).ok_or("HEVC NAL type absent")?;
                if ((byte >> 1) & 63) <= 31 {
                    header = &data[..marker.start()];
                    data = &data[marker.start()..];
                    break;
                }
            }
            require(!header.is_empty(), "HEVC codec header missing")?;
        }
        let mut message = capnp::message::Builder::new_default();
        {
            let mut root = message.init_root::<event::Builder<'_>>();
            root.set_valid(true);
            root.set_log_mono_time(20_000_000_000 + sequence as u64);
            let mut encoded = root.init_road_encode_data();
            encoded.set_width(0);
            encoded.set_height(0);
            encoded.set_header(header);
            encoded.set_data(data);
            let mut index = encoded.get_idx()?;
            index.set_type(encode_index::Type::FullHEVC);
            index.set_segment_num(12);
            index.set_frame_id(sequence as u32);
            index.set_encode_id(sequence as u32);
            index.set_segment_id(sequence as u32);
            index.set_timestamp_eof(1_000_000_000 + sequence as u64 * 50_000_000);
            index.set_flags(
                if packet["flags"].as_str().is_some_and(|v| v.contains('K')) {
                    8
                } else {
                    0
                },
            );
            index.set_len(u32::try_from(data.len())?);
        }
        records.push(capnp::serialize::write_message_to_words(&message));
    }
    require(
        reconstructed == raw,
        "HEVC packet slices do not reconstruct full bitstream",
    )?;
    save(
        &output.join("invocation.json"),
        &json!({"argv":argv,"bitstream_sha256":super::hash(&raw),"packets":records.len()}),
    )?;
    Ok(Sample { raw, records })
}
