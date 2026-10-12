use super::bits::{Reader, Result};
pub(super) struct Info {
    pub chroma: u64,
    pub width_mbs: u64,
    pub height_units: u64,
    pub frame_only: u64,
    pub crop_flag: usize,
    pub after_crop: usize,
    pub crop: [u64; 4],
}
fn scaling(reader: &mut Reader<'_>, size: usize) -> Result<()> {
    let mut last = 8;
    let mut next = 8;
    for _ in 0..size {
        if next != 0 {
            next = (last + reader.se()? + 256).rem_euclid(256);
        }
        if next != 0 {
            last = next;
        }
    }
    Ok(())
}
fn high_profile(reader: &mut Reader<'_>, profile: u64) -> Result<u64> {
    if ![100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135].contains(&profile) {
        return Ok(1);
    }
    let chroma = reader.ue()?;
    let separate = chroma == 3 && reader.bit()? != 0;
    reader.ue()?;
    reader.ue()?;
    reader.bit()?;
    if reader.bit()? != 0 {
        for index in 0..if chroma == 3 { 12 } else { 8 } {
            if reader.bit()? != 0 {
                scaling(reader, if index < 6 { 16 } else { 64 })?;
            }
        }
    }
    Ok(if separate { 0 } else { chroma })
}
pub(super) fn crop(reader: &mut Reader<'_>) -> Result<Info> {
    let profile = reader.bits(8)?;
    reader.bits(8)?;
    reader.bits(8)?;
    reader.ue()?;
    let chroma = high_profile(reader, profile)?;
    reader.ue()?;
    match reader.ue()? {
        0 => {
            reader.ue()?;
        }
        1 => {
            reader.bit()?;
            reader.se()?;
            reader.se()?;
            for _ in 0..reader.ue()? {
                reader.se()?;
            }
        }
        _ => {}
    }
    reader.ue()?;
    reader.bit()?;
    let width_mbs = reader.ue()?;
    let height_units = reader.ue()?;
    let frame_only = reader.bit()?;
    if frame_only == 0 {
        reader.bit()?;
    }
    reader.bit()?;
    let crop_flag = reader.position;
    let mut crop = [0; 4];
    if reader.bit()? != 0 {
        for value in &mut crop {
            *value = reader.ue()?;
        }
    }
    Ok(Info {
        chroma,
        width_mbs,
        height_units,
        frame_only,
        crop_flag,
        after_crop: reader.position,
        crop,
    })
}
pub(super) fn crop_units(chroma: u64, frame_only: u64) -> (u64, u64) {
    match chroma {
        1 => (2, 2 * (2 - frame_only)),
        2 => (2, 2 - frame_only),
        _ => (1, 2 - frame_only),
    }
}
fn hrd(reader: &mut Reader<'_>) -> Result<()> {
    let count = reader.ue()? + 1;
    reader.bits(4)?;
    reader.bits(4)?;
    for _ in 0..count {
        reader.ue()?;
        reader.ue()?;
        reader.bit()?;
    }
    for _ in 0..4 {
        reader.bits(5)?;
    }
    Ok(())
}
pub(super) fn vui_timing(reader: &mut Reader<'_>) -> Result<bool> {
    Ok(vui(reader)?.timing != 0)
}
#[derive(Default)]
pub(super) struct Vui {
    pub timing: u64,
    pub tick: u64,
    pub scale: u64,
    pub fixed: u64,
    pub restriction: u64,
    pub reorder: u64,
    pub max_dpb: u64,
}
pub(super) fn vui(reader: &mut Reader<'_>) -> Result<Vui> {
    let mut info = Vui::default();
    if reader.bit()? != 0 && reader.bits(8)? == 255 {
        reader.bits(16)?;
        reader.bits(16)?;
    }
    if reader.bit()? != 0 {
        reader.bit()?;
    }
    if reader.bit()? != 0 {
        reader.bits(3)?;
        reader.bit()?;
        if reader.bit()? != 0 {
            reader.bits(8)?;
            reader.bits(8)?;
            reader.bits(8)?;
        }
    }
    if reader.bit()? != 0 {
        reader.ue()?;
        reader.ue()?;
    }
    info.timing = reader.bit()?;
    if info.timing != 0 {
        info.tick = reader.bits(32)?;
        info.scale = reader.bits(32)?;
        info.fixed = reader.bit()?;
    }
    let nal = reader.bit()? != 0;
    if nal {
        hrd(reader)?;
    }
    let vcl = reader.bit()? != 0;
    if vcl {
        hrd(reader)?;
    }
    if nal || vcl {
        reader.bit()?;
    }
    reader.bit()?;
    info.restriction = reader.bit()?;
    if info.restriction != 0 {
        reader.bit()?;
        for _ in 0..4 {
            reader.ue()?;
        }
        info.reorder = reader.ue()?;
        info.max_dpb = reader.ue()?;
    }
    Ok(info)
}
