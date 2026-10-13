use crate::Value;

fn encoding(bytes: &[u8]) -> (&[u8], usize, bool) {
    for (bom, width, little) in [
        (&[0xff, 0xfe, 0, 0][..], 4, true),
        (&[0, 0, 0xfe, 0xff][..], 4, false),
        (&[0xff, 0xfe][..], 2, true),
        (&[0xfe, 0xff][..], 2, false),
        (&[0xef, 0xbb, 0xbf][..], 1, false),
    ] {
        if let Some(rest) = bytes.strip_prefix(bom) {
            return (rest, width, little);
        }
    }
    if bytes.len() >= 4 {
        if bytes[0] == 0 {
            return (bytes, if bytes[1] == 0 { 4 } else { 2 }, false);
        }
        if bytes[1] == 0 {
            return (
                bytes,
                if bytes[2] == 0 && bytes[3] == 0 { 4 } else { 2 },
                true,
            );
        }
    } else if bytes.len() == 2 {
        if bytes[0] == 0 {
            return (bytes, 2, false);
        }
        if bytes[1] == 0 {
            return (bytes, 2, true);
        }
    }
    (bytes, 1, false)
}
fn point(output: &mut String, value: u32) -> Result<(), ()> {
    use std::fmt::Write;
    match char::from_u32(value) {
        Some(value) => {
            output.push(value);
            Ok(())
        }
        None if (0xd800..=0xdfff).contains(&value) => {
            write!(output, "\\u{value:04x}").map_err(|_| ())
        }
        None => Err(()),
    }
}
fn utf8(mut bytes: &[u8]) -> Result<String, ()> {
    let mut output = String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                output.push_str(text);
                break;
            }
            Err(error) => {
                let end = error.valid_up_to();
                output.push_str(std::str::from_utf8(&bytes[..end]).map_err(|_| ())?);
                bytes = &bytes[end..];
                let Some([0xed, middle, last, ..]) = bytes.get(..3) else {
                    return Err(());
                };
                if !(0xa0..=0xbf).contains(middle) || !(0x80..=0xbf).contains(last) {
                    return Err(());
                }
                let value = 0xd000 + ((u32::from(*middle) & 0x3f) << 6) + (u32::from(*last) & 0x3f);
                point(&mut output, value)?;
                bytes = &bytes[3..];
            }
        }
    }
    Ok(output)
}
pub(super) fn parse(bytes: &[u8]) -> Result<Value, ()> {
    let (bytes, width, little) = encoding(bytes);
    let text = if width == 1 {
        utf8(bytes)?
    } else {
        if bytes.len() % width != 0 {
            return Err(());
        }
        let mut output = String::new();
        if width == 2 {
            let words = bytes.chunks_exact(2).map(|part| {
                if little {
                    u16::from_le_bytes([part[0], part[1]])
                } else {
                    u16::from_be_bytes([part[0], part[1]])
                }
            });
            for value in char::decode_utf16(words) {
                match value {
                    Ok(value) => output.push(value),
                    Err(error) => point(&mut output, u32::from(error.unpaired_surrogate()))?,
                }
            }
        } else {
            for part in bytes.chunks_exact(4) {
                let bytes = [part[0], part[1], part[2], part[3]];
                point(
                    &mut output,
                    if little {
                        u32::from_le_bytes(bytes)
                    } else {
                        u32::from_be_bytes(bytes)
                    },
                )?;
            }
        }
        output
    };
    Value::parse(&text).map_err(|_| ())
}
