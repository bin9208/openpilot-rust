use std::borrow::Cow;

pub fn decode(bytes: &[u8]) -> Option<Cow<'_, str>> {
    let (bytes, width, little) = if bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        (&bytes[4..], 4, false)
    } else if bytes.starts_with(&[0xff, 0xfe, 0, 0]) {
        (&bytes[4..], 4, true)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        (&bytes[2..], 2, false)
    } else if bytes.starts_with(&[0xff, 0xfe]) {
        (&bytes[2..], 2, true)
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        (&bytes[3..], 1, false)
    } else if bytes.len() >= 4 && bytes[0] == 0 {
        (bytes, if bytes[1] == 0 { 4 } else { 2 }, false)
    } else if bytes.len() >= 4 && bytes[1] == 0 {
        (
            bytes,
            if bytes[2] == 0 && bytes[3] == 0 { 4 } else { 2 },
            true,
        )
    } else if bytes.len() == 2 && bytes[0] == 0 {
        (bytes, 2, false)
    } else if bytes.len() == 2 && bytes[1] == 0 {
        (bytes, 2, true)
    } else {
        (bytes, 1, false)
    };
    if width == 1 {
        return std::str::from_utf8(bytes).ok().map(Cow::Borrowed);
    }
    if bytes.len() % width != 0 {
        return None;
    }
    let text = if width == 2 {
        let units = bytes
            .chunks_exact(2)
            .map(|pair| {
                let pair = [pair[0], pair[1]];
                if little {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                }
            })
            .collect::<Vec<_>>();
        String::from_utf16(&units).ok()?
    } else {
        bytes
            .chunks_exact(4)
            .map(|chunk| {
                let chunk = [chunk[0], chunk[1], chunk[2], chunk[3]];
                char::from_u32(if little {
                    u32::from_le_bytes(chunk)
                } else {
                    u32::from_be_bytes(chunk)
                })
            })
            .collect::<Option<String>>()?
    };
    Some(Cow::Owned(text))
}
