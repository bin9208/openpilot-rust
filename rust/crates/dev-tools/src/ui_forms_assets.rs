//! Original fixed-asset sizing policy; decoding/upload remains mechanical ABI.
use crate::{
    seal::Result,
    ui_forms_input::{hex, Canvas},
};
use std::{io::Read, path::Path};
pub fn load(
    api: &mut dyn Canvas,
    root: &Path,
    id: usize,
    name: &str,
    width: f64,
    height: f64,
    keep: bool,
) -> Result<(f64, f64)> {
    let path = root.join("openpilot/selfdrive/assets").join(name);
    let mut header = [0u8; 24];
    std::fs::File::open(&path)?.read_exact(&mut header)?;
    if header[..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err("forms pinned asset is not PNG; independently recapture asset decode".into());
    }
    let w = f64::from(u32::from_be_bytes(header[16..20].try_into().unwrap()));
    let h = f64::from(u32::from_be_bytes(header[20..24].try_into().unwrap()));
    let logical = (width.round_ties_even(), height.round_ties_even());
    let (width, height) = if api.pixel_scale() != 1.0 {
        (
            (logical.0 * api.pixel_scale()).trunc().min(w),
            (logical.1 * api.pixel_scale()).trunc().min(h),
        )
    } else {
        logical
    };
    let (w, h) = if keep {
        let ratio = (width / w).min(height / h);
        ((w * ratio).trunc(), (h * ratio).trunc())
    } else {
        (width, height)
    };

    api.call(&format!(
        "texture-load {id} {} {w} {h}",
        hex(path.to_str().ok_or("forms asset path UTF8")?)
    ))?;
    if api.pixel_scale() != 1.0 {
        api.call(&format!("texture-size {id} {} {}", logical.0, logical.1))?;
        Ok(logical)
    } else {
        Ok((w, h))
    }
}
