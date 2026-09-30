use crate::Error;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const DIGITS: &[(u32, u32)] = &[
    (48, 57),
    (178, 179),
    (185, 185),
    (1632, 1641),
    (1776, 1785),
    (1984, 1993),
    (2406, 2415),
    (2534, 2543),
    (2662, 2671),
    (2790, 2799),
    (2918, 2927),
    (3046, 3055),
    (3174, 3183),
    (3302, 3311),
    (3430, 3439),
    (3558, 3567),
    (3664, 3673),
    (3792, 3801),
    (3872, 3881),
    (4160, 4169),
    (4240, 4249),
    (4969, 4977),
    (6112, 6121),
    (6160, 6169),
    (6470, 6479),
    (6608, 6618),
    (6784, 6793),
    (6800, 6809),
    (6992, 7001),
    (7088, 7097),
    (7232, 7241),
    (7248, 7257),
    (8304, 8304),
    (8308, 8313),
    (8320, 8329),
    (9312, 9320),
    (9332, 9340),
    (9352, 9360),
    (9450, 9450),
    (9461, 9469),
    (9471, 9471),
    (10102, 10110),
    (10112, 10120),
    (10122, 10130),
    (42528, 42537),
    (43216, 43225),
    (43264, 43273),
    (43472, 43481),
    (43504, 43513),
    (43600, 43609),
    (44016, 44025),
    (65296, 65305),
    (66720, 66729),
    (68160, 68163),
    (68912, 68921),
    (69216, 69224),
    (69714, 69722),
    (69734, 69743),
    (69872, 69881),
    (69942, 69951),
    (70096, 70105),
    (70384, 70393),
    (70736, 70745),
    (70864, 70873),
    (71248, 71257),
    (71360, 71369),
    (71472, 71481),
    (71904, 71913),
    (72016, 72025),
    (72784, 72793),
    (73040, 73049),
    (73120, 73129),
    (73552, 73561),
    (92768, 92777),
    (92864, 92873),
    (93008, 93017),
    (120782, 120831),
    (123200, 123209),
    (123632, 123641),
    (124144, 124153),
    (125264, 125273),
    (127232, 127242),
    (130032, 130041),
];
fn bad(text: &str, status: u16) -> Error {
    Error::Http {
        status,
        text: text.into(),
    }
}
pub fn safe_segment(segment: &str) -> Result<&str, Error> {
    let segment =
        segment.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    let mut parts = segment.split("--");
    parts.next();
    let valid = parts.last().is_some_and(|number| {
        !number.is_empty()
            && number.chars().all(|c| {
                DIGITS
                    .iter()
                    .any(|&(start, end)| (start..=end).contains(&(c as u32)))
            })
    });
    if segment.is_empty()
        || segment.contains(['/', '\\'])
        || [".", ".."].contains(&segment)
        || !valid
    {
        return Err(bad("bad segment", 400));
    }
    Ok(segment)
}
pub fn segment_index(segment: &str) -> i64 {
    let number = segment.split("--").last().unwrap_or("").trim();
    let normalized: Option<String> = number
        .chars()
        .map(|c| {
            if c.is_ascii() {
                return Some(c);
            }
            decimal_digit(c).map(|digit| char::from(b'0' + digit))
        })
        .collect();
    normalized
        .and_then(|value| {
            static INTEGER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let integer = INTEGER.get_or_init(|| {
                regex::Regex::new(r"^[+-]?[0-9](?:_?[0-9])*$").expect("literal integer pattern")
            });
            integer
                .is_match(&value)
                .then(|| value.replace('_', "").parse().ok())
                .flatten()
        })
        .unwrap_or(0)
}
pub fn decimal_digit(c: char) -> Option<u8> {
    static DECIMAL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let decimal =
        DECIMAL.get_or_init(|| regex::Regex::new(r"^\p{Nd}$").expect("literal decimal pattern"));
    if !decimal.is_match(c.encode_utf8(&mut [0; 4])) {
        return None;
    }
    DIGITS.iter().find_map(|&(start, end)| {
        let value = c as u32;
        (start..=end)
            .contains(&value)
            .then(|| ((value - start) % 10) as u8)
    })
}
pub fn route_name(segment: &str) -> String {
    let parts = segment.split("--").collect::<Vec<_>>();
    parts[..parts.len().saturating_sub(1)].join("--")
}
pub fn size_label(size: u64) -> String {
    let n = size as f64;
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", n / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", n / (1024.0 * 1024.0 * 1024.0))
    }
}
pub fn segment_dir(root: &Path, segment: &str) -> Result<PathBuf, Error> {
    let segment = safe_segment(segment)?;
    let root = std::path::absolute(root)?;
    let path = root.join(segment);
    if !path.is_dir() {
        return Err(bad("segment not found", 404));
    }
    Ok(path)
}
#[derive(Clone, Debug, Serialize)]
pub struct SourceFile {
    pub kind: &'static str,
    pub name: String,
    pub size: u64,
    #[serde(rename = "sizeLabel")]
    pub size_label: String,
}
pub const RLOG_NAMES: &[&str] = &["rlog.zst", "rlog.bz2", "rlog"];
pub fn file_summary(directory: &Path) -> Result<Vec<SourceFile>, Error> {
    let mut files = Vec::new();
    for (kind, names) in [
        ("qcamera", &["qcamera.ts", "qcamera.mp4"][..]),
        ("rlog", RLOG_NAMES),
    ] {
        for name in names {
            if let Ok(meta) = fs::metadata(directory.join(name)) {
                if meta.is_file() && meta.len() > 0 {
                    files.push(SourceFile {
                        kind,
                        name: (*name).into(),
                        size: meta.len(),
                        size_label: size_label(meta.len()),
                    });
                    break;
                }
            }
        }
    }
    if !files.iter().any(|file| file.kind == "rlog") {
        return Err(bad("rlog not found", 404));
    }
    Ok(files)
}
pub fn segment_complete(root: &Path, segment: &str) -> bool {
    let Ok(entries) = fs::read_dir(root.join(segment)) else {
        return false;
    };
    let mut has_rlog = false;
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let name = entry.file_name();
        if name.as_encoded_bytes().ends_with(b".lock") {
            return false;
        }
        if RLOG_NAMES.iter().any(|expected| name == *expected) {
            if let Ok(meta) = fs::symlink_metadata(entry.path()) {
                has_rlog = meta.is_file() && meta.len() > 0;
            }
        }
    }
    has_rlog
}
pub fn validate_selection(root: &Path, segments: &[String]) -> Result<Vec<String>, Error> {
    let segments = segments
        .iter()
        .map(|segment| safe_segment(segment).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    if segments.is_empty() {
        return Err(bad("missing segments", 400));
    }
    if let Some(segment) = segments
        .iter()
        .find(|segment| !segment_complete(root, segment))
    {
        return Err(bad(
            &format!("segment is still recording or incomplete: {segment}"),
            409,
        ));
    }
    Ok(segments)
}
