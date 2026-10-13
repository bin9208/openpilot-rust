use super::{
    bits::{self, Reader},
    parse,
};
use std::fmt::Write;

#[must_use]
pub fn info(nal: &[u8]) -> String {
    if nal.len() < 5 || nal[0] & 31 != 7 {
        return String::new();
    }
    describe(nal).unwrap_or_default()
}

fn describe(nal: &[u8]) -> bits::Result<String> {
    let rbsp = bits::unescape(&nal[1..]);
    let mut reader = Reader::new(&rbsp);
    let parsed = parse::crop(&mut reader)?;
    let coded_width = (i128::from(parsed.width_mbs) + 1) * 16;
    let coded_height =
        (i128::from(parsed.height_units) + 1) * 16 * (2 - i128::from(parsed.frame_only));
    let units = parse::crop_units(parsed.chroma, parsed.frame_only);
    let [left, right, top, bottom] = parsed.crop;
    let width = coded_width - (i128::from(left) + i128::from(right)) * i128::from(units.0);
    let height = coded_height - (i128::from(top) + i128::from(bottom)) * i128::from(units.1);
    let mut vui = " vui=0".to_owned();
    if parsed.after_crop < bits::stop(&rbsp)? && reader.bit()? != 0 {
        vui = match parse::vui(&mut reader) {
            Ok(info) => {
                let mut text = format!(
                    " vui=1 timing={} tick={}/{} fixed={} restrict={}",
                    info.timing, info.tick, info.scale, info.fixed, info.restriction
                );
                if info.restriction != 0 {
                    write!(text, " reorder={} max_dpb={}", info.reorder, info.max_dpb)
                        .map_err(|_| "SPS summary formatting failed")?;
                }
                text
            }
            Err(_) => " vui=1 timing=?".to_owned(),
        };
    }
    Ok(format!(
        "profile=0x{:02X} constraints=0x{:02X} level=0x{:02X} coded={coded_width}x{coded_height} \
        display={width}x{height} crop={left},{right},{top},{bottom}{vui}",
        rbsp[0], rbsp[1], rbsp[2]
    ))
}
