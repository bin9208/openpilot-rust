//! Source WAV PCM16 conversion, including NumPy's float32 resampling stages.
use crate::seal::Result;
use std::{fs, path::Path};

pub(super) fn write(path: &Path, rate: u32, channels: u16, pcm: &[u8]) -> Result<()> {
    let len = u32::try_from(pcm.len())?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    let align = channels.checked_mul(2).ok_or("WAV alignment overflow")?;
    bytes.extend_from_slice(
        &rate
            .checked_mul(u32::from(align))
            .ok_or("WAV rate overflow")?
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&align.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(pcm);
    fs::write(path, bytes)?;
    Ok(())
}
fn number(bytes: &[u8], start: usize, len: usize) -> Result<u32> {
    let data = bytes
        .get(start..start.checked_add(len).ok_or("WAV offset overflow")?)
        .ok_or("truncated WAV integer")?;
    Ok(match len {
        2 => u32::from(u16::from_le_bytes(data.try_into()?)),
        4 => u32::from_le_bytes(data.try_into()?),
        _ => return Err("WAV integer width".into()),
    })
}
pub(super) fn read(path: &Path, gain: f64) -> Result<Vec<f32>> {
    let bytes = fs::read(path)?;
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("source requires RIFF WAVE".into());
    }
    let mut format = None;
    let mut pcm = None;
    let mut cursor = 12_usize;
    while cursor.checked_add(8).is_some_and(|end| end <= bytes.len()) {
        let len = usize::try_from(number(&bytes, cursor + 4, 4)?)?;
        let begin = cursor + 8;
        let end = begin.checked_add(len).ok_or("WAV chunk overflow")?;
        let data = bytes.get(begin..end).ok_or("truncated WAV chunk")?;
        match &bytes[cursor..cursor + 4] {
            b"fmt " => {
                format = Some((
                    number(data, 0, 2)?,
                    number(data, 2, 2)?,
                    number(data, 4, 4)?,
                    number(data, 14, 2)?,
                ))
            }
            b"data" => {
                pcm = Some(data);
                break;
            }
            _ => (),
        }
        cursor = end.checked_add(len % 2).ok_or("WAV chunk overflow")?;
    }
    let (encoding, channels, rate, bits) = format.ok_or("missing WAV format")?;
    if encoding != 1 || !matches!(channels, 1 | 2) || bits != 16 || rate == 0 {
        return Err("source sound requires mono/stereo PCM16 WAV".into());
    }
    let pcm = pcm.ok_or("missing WAV data")?;
    let width = usize::try_from(channels)? * 2;
    if !pcm.len().is_multiple_of(width) {
        return Err("partial WAV frame".into());
    }
    let samples: Vec<f64> = pcm
        .chunks_exact(width)
        .map(|frame| {
            let left = f64::from(i16::from_le_bytes([frame[0], frame[1]]));
            if channels == 1 {
                left
            } else {
                left / 2.0 + f64::from(i16::from_le_bytes([frame[2], frame[3]])) / 2.0
            }
        })
        .collect();
    if rate == 48000 {
        return Ok(samples
            .into_iter()
            .map(|sample| ((sample * gain) as f32) / 32768.0)
            .collect());
    }
    let factor = 48000.0 / f64::from(rate);
    let count = (samples.len() as f64 * factor) as usize;
    let mut result = Vec::with_capacity(count);
    for position in 0..count {
        let index = position as f64 / factor;
        let low = index as usize;
        let high = (low + 1).min(samples.len() - 1);
        let interpolated =
            samples[low] * (high as f64 - index) + samples[high] * (index - low as f64);
        // The original first stores into a float32 array, then NumPy casts its
        // weak Python gain scalar to float32, then divides by the exact 32768.
        result.push((interpolated as f32) * (gain as f32) / 32768.0);
    }
    Ok(result)
}
