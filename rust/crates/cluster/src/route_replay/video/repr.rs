use super::Segment;
use crate::route_replay::Error;
use std::{fmt::Write, os::unix::ffi::OsStrExt, path::Path};
use unicode_general_category::{get_general_category, GeneralCategory};
fn printable(point: u32) -> bool {
    point == 32
        || char::from_u32(point).is_some_and(|ch| {
            !matches!(
                get_general_category(ch),
                GeneralCategory::Control
                    | GeneralCategory::Format
                    | GeneralCategory::Surrogate
                    | GeneralCategory::PrivateUse
                    | GeneralCategory::Unassigned
                    | GeneralCategory::LineSeparator
                    | GeneralCategory::ParagraphSeparator
                    | GeneralCategory::SpaceSeparator
            )
        })
}
pub(super) fn path_repr(path: &Path) -> Result<String, Error> {
    // Unix Python decodes file names with UTF-8/surrogateescape, then repr escapes
    // undecodable bytes. Preserve those bytes rather than using lossy text.
    let mut bytes = path.as_os_str().as_bytes();
    let mut points = Vec::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                points.extend(text.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let (valid, tail) = bytes.split_at(error.valid_up_to());
                points.extend(
                    std::str::from_utf8(valid)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .chars()
                        .map(u32::from),
                );
                let count = error.error_len().unwrap_or(tail.len());
                points.extend(tail[..count].iter().map(|&b| 0xdc00 + u32::from(b)));
                bytes = &tail[count..];
            }
        }
    }
    let quote = if points.contains(&39) && !points.contains(&34) {
        34
    } else {
        39
    };
    let mut text = String::new();
    text.push(char::from_u32(quote).expect("ASCII quote"));
    for point in points {
        match point {
            9 => text.push_str("\\t"),
            10 => text.push_str("\\n"),
            13 => text.push_str("\\r"),
            92 => text.push_str("\\\\"),
            p if p == quote => {
                text.push('\\');
                text.push(char::from_u32(p).expect("ASCII quote"));
            }
            p if printable(p) => text.push(char::from_u32(p).expect("printable Unicode scalar")),
            0..=255 => write!(text, "\\x{point:02x}").map_err(|e| Error::Invalid(e.to_string()))?,
            256..=65535 => {
                write!(text, "\\u{point:04x}").map_err(|e| Error::Invalid(e.to_string()))?;
            }
            _ => write!(text, "\\U{point:08x}").map_err(|e| Error::Invalid(e.to_string()))?,
        }
    }
    text.push(char::from_u32(quote).expect("ASCII quote"));
    Ok(text)
}
pub(super) fn key_repr(segment: &Segment) -> Result<String, Error> {
    let mut start = String::new();
    let mut end = String::new();
    openpilot_runtime_core::python_float::write_float(segment.start_t, &mut start)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    openpilot_runtime_core::python_float::write_float(segment.end_t, &mut end)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(format!(
        "({}, {}, {}, {})",
        segment
            .index
            .map_or_else(|| "None".into(), |v| v.to_string()),
        path_repr(&segment.path)?,
        start.replace("NaN", "nan").replace("Infinity", "inf"),
        end.replace("NaN", "nan").replace("Infinity", "inf")
    ))
}

#[cfg(test)]
mod tests {
    use super::{key_repr, path_repr};
    use crate::route_replay::video::Segment;
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::Path};
    #[test]
    fn key_uses_python_float_quote_unicode_and_surrogateescape() {
        assert_eq!(unicode_general_category::UNICODE_VERSION, (15, 0, 0));
        let key = Segment {
            index: None,
            path: "a'한글\u{a0}\u{200d}\n\\\"".into(),
            start_t: -0.0,
            end_t: 1e-5,
        };
        assert_eq!(
            key_repr(&key).unwrap(),
            "(None, 'a\\'한글\\xa0\\u200d\\n\\\\\"', -0.0, 1e-05)"
        );
        assert_eq!(
            path_repr(Path::new(OsStr::from_bytes(b"a\xff\xed\xa0\x80"))).unwrap(),
            "'a\\udcff\\udced\\udca0\\udc80'"
        );
        assert_eq!(path_repr(Path::new("a'b")).unwrap(), "\"a'b\"");
    }
}
