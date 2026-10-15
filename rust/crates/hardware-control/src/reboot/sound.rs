use std::{fs, io, path::Path};

pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}
fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn u16_at(bytes: &[u8], at: usize) -> io::Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or_else(|| bad("truncated WAV"))?
            .try_into()
            .map_err(|_| bad("WAV integer"))?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> io::Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| bad("truncated WAV"))?
            .try_into()
            .map_err(|_| bad("WAV integer"))?,
    ))
}
/// The original reads the raw setting independently of Params initialization.
pub fn volume(setting: Option<&str>) -> io::Result<f64> {
    let number = match setting.and_then(|s| openpilot_hardware_info::parse_integer(s).ok()) {
        Some(value) => value.to_f64().map_err(io::Error::other)? / 100.0,
        None => 1.0,
    };
    Ok((0.5 * number).clamp(0.0, 1.0))
}
pub fn load(path: &Path, volume: f64) -> io::Result<Pcm> {
    let bytes = fs::read(path)?;
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(bad("not a RIFF WAVE asset"));
    }
    let mut offset = 12usize;
    let mut format = None;
    let mut data = None;
    while offset.checked_add(8).is_some_and(|end| end <= bytes.len()) {
        let size = usize::try_from(u32_at(&bytes, offset + 4)?).map_err(|_| bad("WAV size"))?;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .ok_or_else(|| bad("WAV size overflow"))?;
        let chunk = bytes
            .get(start..end)
            .ok_or_else(|| bad("truncated WAV chunk"))?;
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                let encoding = u16_at(chunk, 0)?;
                let channels = u16_at(chunk, 2)?;
                let rate = u32_at(chunk, 4)?;
                let bits = u16_at(chunk, 14)?;
                let pcm = encoding == 1
                    || (encoding == 0xfffe
                        && chunk.get(24..40)
                            == Some(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]));
                if !pcm || channels == 0 || rate == 0 || bits != 16 {
                    return Err(bad("Expected 16-bit reboot sound"));
                }
                format = Some((channels, rate));
            }
            b"data" => {
                if format.is_none() {
                    return Err(bad("WAV data before format"));
                }
                data = Some(chunk);
                break;
            }
            _ => {}
        }
        offset = end
            .checked_add(size % 2)
            .ok_or_else(|| bad("WAV offset overflow"))?;
    }
    let (channels, rate) = format.ok_or_else(|| bad("missing WAV format"))?;
    let data = data.ok_or_else(|| bad("missing WAV data"))?;
    if !data.len().is_multiple_of(usize::from(channels) * 2) {
        return Err(bad("partial WAV frame"));
    }
    // Source truncates rate*0.15 and keeps the original channel count and rate.
    let lead = (f64::from(rate) * 0.15) as usize;
    let mut samples = vec![
        0.0;
        lead.checked_mul(usize::from(channels))
            .ok_or_else(|| bad("WAV lead-in overflow"))?
    ];
    let gain = volume as f32;
    samples.extend(
        data.chunks_exact(2)
            .map(|v| (f32::from(i16::from_le_bytes([v[0], v[1]])) / 32768.0) * gain),
    );
    Ok(Pcm {
        rate,
        channels,
        samples,
    })
}
