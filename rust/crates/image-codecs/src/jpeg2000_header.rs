//! Pillow 12.3 JP2/J2K mode selection before native sample decoding.
use crate::DecodeError;
type Result<T> = std::result::Result<T, DecodeError>;
#[derive(Debug)]
pub struct Header {
    pub mode: &'static str,
    pub width: u32,
    pub height: u32,
    pub palette: bool,
}
fn range(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    bytes
        .get(
            at..at
                .checked_add(len)
                .ok_or_else(|| DecodeError("JPEG2000 header overflow".into()))?,
        )
        .ok_or_else(|| DecodeError("JPEG2000 header truncated".into()))
}
fn u16at(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(range(bytes, at, 2)?.try_into().unwrap()))
}
fn u32at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(range(bytes, at, 4)?.try_into().unwrap()))
}
fn mode(components: u16, precision: u8, raw: bool) -> Result<&'static str> {
    Ok(match components {
        1 => {
            if if raw {
                u16::from(precision & 127) + 1 > 8
            } else {
                precision & 127 > 8
            } {
                "I;16"
            } else {
                "L"
            }
        }
        2 => "LA",
        3 => "RGB",
        4 => "RGBA",
        _ => return Err(DecodeError("JPEG2000 component count unsupported".into())),
    })
}
fn boxes(bytes: &[u8]) -> Result<Vec<(&[u8], &[u8])>> {
    let mut at = 0usize;
    let mut out = Vec::new();
    while at < bytes.len() {
        let size = u32at(bytes, at)?;
        let typ = range(bytes, at + 4, 4)?;
        let (length, header) = if size == 1 {
            (
                usize::try_from(u64::from_be_bytes(
                    range(bytes, at + 8, 8)?.try_into().unwrap(),
                ))
                .map_err(|_| DecodeError("JPEG2000 box length overflow".into()))?,
                16,
            )
        } else {
            (size as usize, 8)
        };
        if length < header {
            return Err(DecodeError("JPEG2000 box length invalid".into()));
        }
        out.push((typ, range(bytes, at + header, length - header)?));
        at = at
            .checked_add(length)
            .ok_or_else(|| DecodeError("JPEG2000 box overflow".into()))?;
    }
    Ok(out)
}
pub fn parse(bytes: &[u8]) -> Result<Header> {
    if bytes.starts_with(&[255, 79, 255, 81]) {
        let siz = range(bytes, 4, usize::from(u16at(bytes, 4)?))?;
        if siz.len() < 38 {
            return Err(DecodeError("JPEG2000 SIZ length invalid".into()));
        }
        let width = u32at(siz, 4)?
            .checked_sub(u32at(siz, 12)?)
            .ok_or_else(|| DecodeError("JPEG2000 SIZ width invalid".into()))?;
        let height = u32at(siz, 8)?
            .checked_sub(u32at(siz, 16)?)
            .ok_or_else(|| DecodeError("JPEG2000 SIZ height invalid".into()))?;
        let components = u16at(siz, 36)?;
        let precision = *range(siz, 38, 1)?.first().unwrap();
        return Ok(Header {
            mode: mode(components, precision, true)?,
            width,
            height,
            palette: false,
        });
    }
    if !bytes.starts_with(b"\0\0\0\x0cjP  \r\n\x87\n") {
        return Err(DecodeError("JPEG2000 signature invalid".into()));
    }
    let mut width = 0;
    let mut height = 0;
    let mut selected = None;
    let mut components = 0;
    let mut color = None;
    let mut palette = false;
    // Pillow stops searching outer boxes as soon as jp2h is found; trailing
    // bytes and codec stream validation belong to OpenJPEG, not this parser.
    let mut at = 0usize;
    let body = loop {
        let size = u32at(bytes, at)?;
        let typ = range(bytes, at + 4, 4)?;
        let (length, header) = if size == 1 {
            (
                usize::try_from(u64::from_be_bytes(
                    range(bytes, at + 8, 8)?.try_into().unwrap(),
                ))
                .map_err(|_| DecodeError("JPEG2000 box overflow".into()))?,
                16,
            )
        } else {
            (size as usize, 8)
        };
        if length < header {
            return Err(DecodeError("JPEG2000 box length invalid".into()));
        }
        let body = range(bytes, at + header, length - header)?;
        if typ == b"jp2h" {
            break body;
        }
        at = at
            .checked_add(length)
            .ok_or_else(|| DecodeError("JPEG2000 box offset overflow".into()))?;
    };
    for (typ, body) in boxes(body)? {
        match typ {
            b"ihdr" => {
                height = u32at(body, 0)?;
                width = u32at(body, 4)?;
                components = u16at(body, 8)?;
                selected = Some(mode(
                    components,
                    *range(body, 10, 1)?.first().unwrap(),
                    false,
                )?);
            }
            b"colr" => {
                if range(body, 0, 1)? == [1] {
                    let value = u32at(body, 3)?;
                    color = Some(value);
                    if value == 12 && components == 4 {
                        selected = Some("CMYK")
                    }
                }
            }
            b"pclr" => {
                if matches!(selected, Some("L" | "LA")) && !matches!(color, Some(0 | 15 | 17)) {
                    let channels = usize::from(*range(body, 2, 1)?.first().unwrap());
                    let depth = range(body, 3, channels)?;
                    if depth.iter().copied().max().unwrap_or(0) <= 8 {
                        selected = Some(if selected == Some("L") { "P" } else { "PA" });
                        palette = true;
                    }
                }
            }
            _ => {}
        }
    }
    let mode = selected.ok_or_else(|| DecodeError("JPEG2000 image header missing".into()))?;
    if width == 0 || height == 0 {
        return Err(DecodeError("JPEG2000 dimensions invalid".into()));
    }
    Ok(Header {
        mode,
        width,
        height,
        palette,
    })
}
