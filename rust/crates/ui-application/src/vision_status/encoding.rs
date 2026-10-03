use super::Error;

fn point(output: &mut String, value: u32) -> Result<(), Error> {
    if let Some(value) = char::from_u32(value) {
        output.push(value);
        Ok(())
    } else if (0xd800..=0xdfff).contains(&value) {
        if output
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count()
            % 2
            != 0
        {
            return Err(Error::Encoding);
        }
        use std::fmt::Write;
        write!(output, "\\u{value:04x}").map_err(|_| Error::Encoding)
    } else {
        Err(Error::Encoding)
    }
}
pub(super) fn decode(bytes: &[u8]) -> Result<String, Error> {
    enum Encoding {
        Utf8,
        Utf16(bool),
        Utf32(bool),
    }
    let (encoding, bytes) = if bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        (Encoding::Utf32(false), &bytes[4..])
    } else if bytes.starts_with(&[0xff, 0xfe, 0, 0]) {
        (Encoding::Utf32(true), &bytes[4..])
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        (Encoding::Utf16(false), &bytes[2..])
    } else if bytes.starts_with(&[0xff, 0xfe]) {
        (Encoding::Utf16(true), &bytes[2..])
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        (Encoding::Utf8, &bytes[3..])
    } else if bytes.len() >= 4 && bytes[0] == 0 {
        (
            if bytes[1] == 0 {
                Encoding::Utf32(false)
            } else {
                Encoding::Utf16(false)
            },
            bytes,
        )
    } else if bytes.len() >= 4 && bytes[1] == 0 {
        (
            if bytes[2] == 0 && bytes[3] == 0 {
                Encoding::Utf32(true)
            } else {
                Encoding::Utf16(true)
            },
            bytes,
        )
    } else if bytes.len() == 2 && bytes[0] == 0 {
        (Encoding::Utf16(false), bytes)
    } else if bytes.len() == 2 && bytes[1] == 0 {
        (Encoding::Utf16(true), bytes)
    } else {
        (Encoding::Utf8, bytes)
    };
    let mut output = String::new();
    match encoding {
        Encoding::Utf8 => {
            let mut remaining = bytes;
            while !remaining.is_empty() {
                match std::str::from_utf8(remaining) {
                    Ok(value) => {
                        output.push_str(value);
                        break;
                    }
                    Err(error) => {
                        let (valid, invalid) = remaining.split_at(error.valid_up_to());
                        output.push_str(std::str::from_utf8(valid).map_err(|_| Error::Encoding)?);
                        if invalid.len() < 3
                            || invalid[0] != 0xed
                            || !(0xa0..=0xbf).contains(&invalid[1])
                            || !(0x80..=0xbf).contains(&invalid[2])
                        {
                            return Err(Error::Encoding);
                        }
                        let codepoint = u32::from(invalid[0] & 0x0f) << 12
                            | u32::from(invalid[1] & 0x3f) << 6
                            | u32::from(invalid[2] & 0x3f);
                        point(&mut output, codepoint)?;
                        remaining = &invalid[3..];
                    }
                }
            }
        }
        Encoding::Utf16(little) => {
            if bytes.len() % 2 != 0 {
                return Err(Error::Encoding);
            }
            let words = bytes.chunks_exact(2).map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            });
            for value in char::decode_utf16(words) {
                match value {
                    Ok(value) => output.push(value),
                    Err(error) => point(&mut output, u32::from(error.unpaired_surrogate()))?,
                }
            }
        }
        Encoding::Utf32(little) => {
            if bytes.len() % 4 != 0 {
                return Err(Error::Encoding);
            }
            for word in bytes.chunks_exact(4) {
                let word = [word[0], word[1], word[2], word[3]];
                point(
                    &mut output,
                    if little {
                        u32::from_le_bytes(word)
                    } else {
                        u32::from_be_bytes(word)
                    },
                )?;
            }
        }
    }
    Ok(output)
}
