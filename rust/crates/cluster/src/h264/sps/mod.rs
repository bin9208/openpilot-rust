mod bits;
mod parse;
mod summary;
use bits::{Reader, Writer};
pub use summary::info;

#[must_use]
pub fn start_codes(data: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut index = 0;
    while data.len() - index >= 3 {
        let length = if data[index..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if data[index..].starts_with(&[0, 0, 1]) {
            3
        } else {
            0
        };
        if length == 0 {
            index += 1;
        } else {
            starts.push((index, length));
            index += length;
        }
    }
    starts
}
#[must_use]
pub fn nals(data: &[u8]) -> Vec<(usize, usize, usize)> {
    let starts = start_codes(data);
    let mut values = Vec::new();
    for (index, &(start, length)) in starts.iter().enumerate() {
        let begin = start + length;
        let mut end = starts.get(index + 1).map_or(data.len(), |next| next.0);
        while end > begin && data[end - 1] == 0 {
            end -= 1;
        }
        if begin < end {
            values.push((start, begin, end));
        }
    }
    values
}
#[must_use]
pub fn byte_stream_units(data: &[u8]) -> Vec<Vec<u8>> {
    let starts = start_codes(data);
    starts
        .iter()
        .enumerate()
        .map(|(index, &(start, _))| {
            data[start..starts.get(index + 1).map_or(data.len(), |next| next.0)].to_vec()
        })
        .collect()
}
#[must_use]
pub fn first_unit(data: &[u8], kind: u8) -> Option<Vec<u8>> {
    nals(data)
        .into_iter()
        .find(|&(_, start, _)| data[start] & 31 == kind)
        .map(|(begin, _, end)| data[begin..end].to_vec())
}
#[must_use]
pub fn patch_constraints(data: &[u8]) -> (Vec<u8>, bool) {
    let mut patched = data.to_vec();
    let mut changed = false;
    for (_, start, end) in nals(data) {
        if end - start >= 4
            && data[start] & 31 == 7
            && data[start + 1] == 0x42
            && patched[start + 2] & 0x40 == 0
        {
            patched[start + 2] |= 0x40;
            changed = true;
        }
    }
    (patched, changed)
}
fn cropped(nal: &[u8], width: i64, height: i64) -> bits::Result<Option<(Vec<u8>, String)>> {
    let rbsp = bits::unescape(&nal[1..]);
    let mut reader = Reader::new(&rbsp);
    let info = parse::crop(&mut reader)?;
    let coded_width = (info.width_mbs + 1) * 16;
    let coded_height = (info.height_units + 1) * 16 * (2 - info.frame_only);
    let (unit_x, unit_y) = parse::crop_units(info.chroma, info.frame_only);
    if i128::from(width) > i128::from(coded_width) || i128::from(height) > i128::from(coded_height)
    {
        return Ok(None);
    }
    let right_pixels = u64::try_from(i128::from(coded_width) - i128::from(width))
        .map_err(|_| "SPS crop overflow")?;
    let bottom_pixels = u64::try_from(i128::from(coded_height) - i128::from(height))
        .map_err(|_| "SPS crop overflow")?;
    if !right_pixels.is_multiple_of(unit_x) || !bottom_pixels.is_multiple_of(unit_y) {
        return Ok(None);
    }
    let crop = [0, right_pixels / unit_x, 0, bottom_pixels / unit_y];
    if crop == info.crop {
        return Ok(None);
    }
    let mut writer = Writer::default();
    writer.copy(&rbsp, 0, info.crop_flag)?;
    let enabled = crop.iter().any(|&value| value != 0);
    writer.bit(enabled);
    if enabled {
        for value in crop {
            writer.ue(value)?;
        }
    }
    writer.copy(&rbsp, info.after_crop, bits::stop(&rbsp)?)?;
    writer.bit(true);
    let mut patched = vec![nal[0]];
    patched.extend(bits::escape(&writer.bytes()));
    Ok(Some((
        patched,
        format!(
            "coded={coded_width}x{coded_height} display={width}x{height} crop=0,{},0,{}",
            crop[1], crop[3]
        ),
    )))
}
fn write_timing(writer: &mut Writer, fps: i64) -> bits::Result<()> {
    for _ in 0..4 {
        writer.bit(false);
    }
    writer.bit(true);
    writer.bits(1, 32);
    writer.bits(
        u64::try_from((i128::from(fps) * 2).max(2)).map_err(|_| "SPS time scale overflow")?,
        32,
    );
    writer.bit(true);
    for _ in 0..3 {
        writer.bit(false);
    }
    writer.bit(true);
    writer.bit(true);
    for value in [0, 0, 10, 10, 0, 4] {
        writer.ue(value)?;
    }
    Ok(())
}
fn timed(nal: &[u8], fps: i64) -> bits::Result<Option<(Vec<u8>, String)>> {
    let rbsp = bits::unescape(&nal[1..]);
    let mut reader = Reader::new(&rbsp);
    let info = parse::crop(&mut reader)?;
    let stop = bits::stop(&rbsp)?;
    if info.after_crop > stop {
        return Ok(None);
    }
    let mut existing_vui = 0;
    let mut existing_timing = false;
    if info.after_crop < stop {
        reader.position = info.after_crop;
        existing_vui = reader.bit()?;
        if existing_vui != 0 {
            existing_timing = parse::vui_timing(&mut reader).unwrap_or(false);
            if existing_timing {
                return Ok(None);
            }
        }
    }
    let mut writer = Writer::default();
    writer.copy(&rbsp, 0, info.after_crop)?;
    writer.bit(true);
    write_timing(&mut writer, fps)?;
    writer.bit(true);
    let mut patched = vec![nal[0]];
    patched.extend(bits::escape(&writer.bytes()));
    Ok(Some((
        patched,
        format!(
            "fps={fps} num_units_in_tick=1 time_scale={} replaced_vui={existing_vui} previous_timing={}",
            (i128::from(fps) * 2).max(2),
            u8::from(existing_timing)
        ),
    )))
}
fn patch(
    data: &[u8],
    mut transform: impl FnMut(&[u8]) -> bits::Result<Option<(Vec<u8>, String)>>,
) -> (Vec<u8>, bool, String) {
    let mut out = Vec::new();
    let mut last = 0;
    let mut changed = false;
    let mut information = String::new();
    for (_, start, end) in nals(data) {
        let nal = &data[start..end];
        if nal.len() < 5 || nal[0] & 31 != 7 {
            continue;
        }
        if let Ok(Some((patched, info))) = transform(nal) {
            out.extend_from_slice(&data[last..start]);
            out.extend(patched);
            last = end;
            changed = true;
            information = info;
        }
    }
    if !changed {
        return (data.to_vec(), false, String::new());
    }
    out.extend_from_slice(&data[last..]);
    (out, true, information)
}
#[must_use]
pub fn patch_crop(data: &[u8], width: i64, height: i64) -> (Vec<u8>, bool, String) {
    patch(data, |nal| cropped(nal, width, height))
}
#[must_use]
pub fn patch_vui(data: &[u8], fps: i64) -> (Vec<u8>, bool, String) {
    patch(data, |nal| timed(nal, fps))
}
#[derive(Default)]
pub struct PatchCache {
    sps: Option<(Vec<u8>, Vec<u8>)>,
}
impl PatchCache {
    #[must_use]
    pub fn prepare(
        &mut self,
        packet: Vec<u8>,
        width: i64,
        height: i64,
        fps: i64,
        may_have_sps: bool,
    ) -> Vec<u8> {
        if !may_have_sps {
            return packet;
        }
        if let Some((raw, patched)) = &self.sps {
            if let Some(index) = packet.windows(raw.len()).position(|window| window == raw) {
                let mut next = Vec::new();
                next.extend_from_slice(&packet[..index]);
                next.extend_from_slice(patched);
                next.extend_from_slice(&packet[index + raw.len()..]);
                if next != packet {
                    return next;
                }
            }
        }
        let Some(raw) = first_unit(&packet, 7) else {
            return packet;
        };
        let (next, _) = patch_constraints(&packet);
        let (next, _, _) = patch_crop(&next, width, height);
        let (next, _, _) = patch_vui(&next, fps);
        if let Some(patched) = first_unit(&next, 7) {
            self.sps = Some((raw, patched));
        }
        next
    }
}
