use crate::Error;
use num_traits::ToPrimitive;
use std::path::Path;
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or(Error::Contract("truncated WAV"))?
            .try_into()
            .map_err(|_| Error::Contract("WAV integer"))?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or(Error::Contract("truncated WAV"))?
            .try_into()
            .map_err(|_| Error::Contract("WAV integer"))?,
    ))
}
pub fn load(path: &Path, volume: f64) -> Result<Vec<f32>, Error> {
    let bytes = std::fs::read(path)?;
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(Error::Contract("not a RIFF WAVE asset"));
    }
    let mut offset = 12_usize;
    let mut format = None;
    let mut data = None;
    while offset.checked_add(8).is_some_and(|end| end <= bytes.len()) {
        let size = usize::try_from(u32_at(&bytes, offset + 4)?)
            .map_err(|_| Error::Contract("WAV size"))?;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .ok_or(Error::Contract("WAV size overflow"))?;
        let chunk = bytes
            .get(start..end)
            .ok_or(Error::Contract("truncated WAV chunk"))?;
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                format = Some((
                    u16_at(chunk, 0)?,
                    u16_at(chunk, 2)?,
                    u32_at(chunk, 4)?,
                    u16_at(chunk, 14)?,
                ));
            }
            b"data" => {
                data = Some(chunk);
                break;
            }
            _ => (),
        }
        offset = end
            .checked_add(size % 2)
            .ok_or(Error::Contract("WAV offset overflow"))?;
    }
    let (encoding, channels, rate, bits) = format.ok_or(Error::Contract("missing WAV fmt"))?;
    if encoding != 1 || !matches!(channels, 1 | 2) || bits != 16 || rate == 0 {
        return Err(Error::Contract("sound requires mono/stereo PCM16 WAV"));
    }
    let data = data.ok_or(Error::Contract("missing WAV data"))?;
    let width = usize::from(channels) * 2;
    if data.len() % width != 0 {
        return Err(Error::Contract("partial WAV frame"));
    }
    let samples: Vec<f64> = data
        .chunks_exact(width)
        .map(|frame| {
            let left = f64::from(i16::from_le_bytes([frame[0], frame[1]]));
            if channels == 2 {
                left / 2. + f64::from(i16::from_le_bytes([frame[2], frame[3]])) / 2.
            } else {
                left
            }
        })
        .collect();
    if rate == 48000 {
        return samples
            .into_iter()
            .map(|sample| {
                Ok((sample * volume)
                    .to_f32()
                    .ok_or(Error::Contract("sample conversion"))?
                    / 32768.)
            })
            .collect();
    }
    let factor = 48000. / f64::from(rate);
    let count = (samples
        .len()
        .to_f64()
        .ok_or(Error::Contract("sample count"))?
        * factor)
        .to_usize()
        .ok_or(Error::Contract("resampled count"))?;
    let mut output = Vec::with_capacity(count);
    let gain = volume.to_f32().ok_or(Error::Contract("gain conversion"))?;
    for index in 0..count {
        let original = index.to_f64().ok_or(Error::Contract("sample index"))? / factor;
        let lower = original.to_usize().ok_or(Error::Contract("sample index"))?;
        let upper = (lower + 1).min(samples.len() - 1);
        let sample = samples[lower]
            * (upper.to_f64().ok_or(Error::Contract("sample index"))? - original)
            + samples[upper]
                * (original - lower.to_f64().ok_or(Error::Contract("sample index"))?);
        output.push(
            sample
                .to_f32()
                .ok_or(Error::Contract("sample conversion"))?
                * gain
                / 32768.,
        );
    }
    Ok(output)
}
