//! The two computations embedded in teleoprtc/scripts/bump_tag.sh.
//! Original teleoprtc MIT licensing applies; branch/edit/commit/tag remain shell-owned.
use num_bigint::BigInt;
use std::{
    fmt,
    io::{Read, Write},
    path::Path,
};

#[derive(Debug)]
pub struct Error {
    pub class: &'static str,
    pub message: String,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.class, self.message)
    }
}
impl std::error::Error for Error {}
fn error(class: &'static str, message: impl Into<String>) -> Error {
    Error {
        class,
        message: message.into(),
    }
}
type Result<T> = std::result::Result<T, Error>;

/// PEP 621 version lookup. Missing keys, malformed TOML and wrong project
/// containers fail; incidental non-string version values are explicitly outside
/// this release-tool interface rather than silently coerced to another version.
pub fn lookup(bytes: &[u8]) -> Result<Vec<u8>> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| error("UnicodeDecodeError", e.to_string()))?;
    let doc: toml::Value =
        toml::from_str(text).map_err(|e| error("TOMLDecodeError", e.to_string()))?;
    let project = doc
        .get("project")
        .ok_or_else(|| error("KeyError", "'project'"))?;
    let project = project.as_table().ok_or_else(|| {
        error(
            "TypeError",
            "project value does not support string subscripting",
        )
    })?;
    let version = project
        .get("version")
        .ok_or_else(|| error("KeyError", "'version'"))?;
    let version=version.as_str().ok_or_else(||error("UnsupportedVersionType","PEP 621 project.version must be a string; incidental tomllib print(non-string) is not implemented"))?;
    Ok(format!("{version}\n").into_bytes())
}
/// Unix Python stdin ends a line at LF; embedded CR remains input to int().
pub fn bump(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty() {
        return Err(error("EOFError", "EOF when reading a line"));
    }
    let line = &bytes[..bytes
        .iter()
        .position(|b| *b == b'\n')
        .unwrap_or(bytes.len())];
    // Unix Python stdin uses surrogateescape; invalid bytes cannot form decimal digits.
    let split = line.iter().rposition(|&b| b == b'.').map_or(0, |i| i + 1);
    let prefix = &line[..split];
    let tail = std::str::from_utf8(&line[split..])
        .map_err(|_| error("ValueError", "invalid literal for int() with base 10"))?;
    let tail = tail.trim_matches(char::is_whitespace);
    let normalized = tail
        .chars()
        .map(|c| crate::argparse_scan::decimal_value(c).map_or(c, |d| char::from(b'0' + d)))
        .collect::<Vec<_>>();
    let mut digits = String::new();
    for (i, &c) in normalized.iter().enumerate() {
        if c == '_' {
            if i == 0
                || !normalized[i - 1].is_ascii_digit()
                || !normalized.get(i + 1).is_some_and(char::is_ascii_digit)
            {
                return Err(error(
                    "ValueError",
                    "invalid literal for int() with base 10",
                ));
            }
        } else if c.is_ascii_digit() || i == 0 && matches!(c, '+' | '-') {
            digits.push(c)
        } else {
            return Err(error(
                "ValueError",
                "invalid literal for int() with base 10",
            ));
        }
    }
    if digits.bytes().filter(u8::is_ascii_digit).count() > 4300 {
        return Err(error(
            "ValueError",
            "Exceeds the limit (4300 digits) for integer string conversion",
        ));
    }
    let value = digits
        .parse::<BigInt>()
        .map_err(|_| error("ValueError", "invalid literal for int() with base 10"))?
        + BigInt::from(1);
    let value = value.to_string();
    if value.trim_start_matches('-').len() > 4300 {
        return Err(error(
            "ValueError",
            "Exceeds the limit (4300 digits) for integer string conversion",
        ));
    }
    let mut output = prefix.to_vec();
    output.extend_from_slice(format!("{value}\n").as_bytes());
    Ok(output)
}
/// Parent-owned bin delegates here. No Git/sed/tag/commit operations are performed.
pub fn cli() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let op = args
        .next()
        .ok_or("usage: teleop-version lookup FILE | bump")?;
    let output = if op == "lookup" {
        let path = args.next().ok_or("lookup requires FILE")?;
        if args.next().is_some() {
            return Err("unexpected lookup argument".into());
        }
        let bytes = std::fs::read(Path::new(&path)).map_err(|e| {
            error(
                match e.kind() {
                    std::io::ErrorKind::NotFound => "FileNotFoundError",
                    std::io::ErrorKind::PermissionDenied => "PermissionError",
                    std::io::ErrorKind::IsADirectory => "IsADirectoryError",
                    _ => "OSError",
                },
                e.to_string(),
            )
        })?;
        lookup(&bytes)?
    } else if op == "bump" {
        if args.next().is_some() {
            return Err("unexpected bump argument".into());
        }
        let mut bytes = Vec::new();
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        loop {
            let mut byte = [0];
            if input.read(&mut byte)? == 0 {
                break;
            }
            bytes.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        bump(&bytes)?
    } else {
        return Err("usage: teleop-version lookup FILE | bump".into());
    };
    std::io::stdout().write_all(&output)?;
    Ok(())
}
