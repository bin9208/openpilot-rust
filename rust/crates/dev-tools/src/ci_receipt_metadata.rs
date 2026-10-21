//! Small workflow metadata operations, not a Params provider or Python oracle.
//! JSON fields are strings by contract; prefix accepts the validated stage receipt
//! schema only. Tracebacks and arbitrary Python print(non-string) are not emulated.
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    ffi::OsString,
    fmt, fs,
    io::{Read, Write},
    os::unix::ffi::OsStrExt,
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
type Result<T> = std::result::Result<T, Error>;
fn error(class: &'static str, message: impl Into<String>) -> Error {
    Error {
        class,
        message: message.into(),
    }
}
fn io_error(e: std::io::Error) -> Error {
    error(
        match e.kind() {
            std::io::ErrorKind::NotFound => "FileNotFoundError",
            std::io::ErrorKind::PermissionDenied => "PermissionError",
            std::io::ErrorKind::IsADirectory => "IsADirectoryError",
            _ => "OSError",
        },
        e.to_string(),
    )
}
fn label(name: &str) -> Result<Vec<u8>> {
    if name.is_empty()
        || !name
            .bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_uppercase() || i > 0 && c.is_ascii_digit())
    {
        return Err(error(
            "ArgumentError",
            "ENV must be an uppercase shell variable name",
        ));
    }
    Ok(format!("{name}=").into_bytes())
}
/// Explicit workflow receipt admission bound; not arbitrary json.load parity.
pub const MAX_RECEIPT_BYTES: usize = 4 * 1024 * 1024;
fn check_size(size: u64) -> Result<()> {
    if size > MAX_RECEIPT_BYTES as u64 {
        return Err(error(
            "ReceiptTooLarge",
            "receipt exceeds the 4 MiB workflow schema limit",
        ));
    }
    Ok(())
}
/// Follow symlinks as the original open/read_text calls do. Check the opened
/// file first, then limit the actual read too, including growth after metadata.
pub fn read_receipt(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(io_error)?;
    check_size(file.metadata().map_err(io_error)?.len())?;
    let mut bytes = Vec::new();
    file.take(MAX_RECEIPT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    check_size(bytes.len() as u64)?;
    Ok(bytes)
}
fn field(bytes: &[u8], key: &str, prefix: bool) -> Result<Vec<u8>> {
    check_size(bytes.len() as u64)?;
    let text =
        std::str::from_utf8(bytes).map_err(|e| error("UnicodeDecodeError", e.to_string()))?;
    let doc = JsonValue::parse(text).map_err(|e| {
        let class = if matches!(e, openpilot_log_format::JsonError::IntegerLimit) {
            "ValueError"
        } else {
            "JSONDecodeError"
        };
        error(class, e.to_string())
    })?;
    if !doc.is_object() {
        return Err(error("TypeError", "receipt must be an object"));
    }
    let value = doc
        .get(key)
        .ok_or_else(|| error("KeyError", format!("'{key}'")))?;
    let JsonView::Text(points) = value.view() else {
        return Err(error(
            if prefix {
                "UnsupportedReceiptType"
            } else {
                "TypeError"
            },
            "selected field must be a string",
        ));
    };
    // Python stdout on Unix uses UTF-8 + surrogateescape, not replacement text.
    let mut output = Vec::new();
    for &point in points {
        if (0xdc80..=0xdcff).contains(&point) {
            output.push((point - 0xdc00) as u8);
        } else if let Some(c) = char::from_u32(point) {
            let mut b = [0; 4];
            output.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
        } else {
            return Err(error(
                "UnicodeEncodeError",
                "stdout cannot encode selected lone surrogate",
            ));
        }
    }
    Ok(output)
}
pub fn module(bytes: &[u8], env: &str) -> Result<Vec<u8>> {
    let mut out = label(env)?;
    out.extend(field(bytes, "module", false)?);
    out.push(b'\n');
    Ok(out)
}
pub fn prefix(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = field(bytes, "prefix", true)?;
    out.push(b'\n');
    Ok(out)
}
/// PosixPath's lexical normalization: collapse separators and '.', retain '..'
/// and the special exactly-two-leading-slashes form. No symlink resolution.
fn pathlib_bytes(path: &Path) -> Vec<u8> {
    let raw = path.as_os_str().as_bytes();
    let leading = raw.iter().take_while(|&&b| b == b'/').count();
    let mut out = vec![
        b'/';
        if leading == 2 {
            2
        } else {
            usize::from(leading > 0)
        }
    ];
    for part in raw
        .split(|&b| b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if !out.is_empty() && out.last() != Some(&b'/') {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    out
}
fn select_binding(
    directory: &Path,
    env: &str,
    names: impl IntoIterator<Item = Result<OsString>>,
) -> Result<Vec<u8>> {
    let mut out = label(env)?;
    for name in names {
        let name = name?;
        let bytes = name.as_bytes();
        if bytes.starts_with(b"params_pyx") && bytes.ends_with(b".so") {
            out.extend(pathlib_bytes(&directory.join(name)));
            out.push(b'\n');
            return Ok(out);
        }
    }
    Err(error("StopIteration", "no params_pyx*.so match"))
}
/// Replay an independently captured directory enumeration. The ordering is an
/// input supplied by the filesystem provider, never a lexical sort policy.
/// Production first_binding and source-observation tests use the same selector.
pub fn first_binding_from_names(
    directory: &Path,
    env: &str,
    names: &[OsString],
) -> Result<Vec<u8>> {
    select_binding(directory, env, names.iter().cloned().map(Ok))
}
/// pathlib glob does not sort and matches names regardless of file type. A
/// dangling symlink or directory ending .so is a match. Missing/not-directory/
/// unreadable directories produce the original next(...): StopIteration.
pub fn first_binding(directory: &Path, env: &str) -> Result<Vec<u8>> {
    match fs::read_dir(directory) {
        Ok(entries) => select_binding(
            directory,
            env,
            entries.map(|entry| entry.map(|e| e.file_name()).map_err(io_error)),
        ),
        Err(_) => select_binding(directory, env, std::iter::empty()),
    }
}
/// Parent-owned bin should print this error to stderr and exit 1. All bytes are
/// computed before writing stdout, so rejected receipts publish no partial ENV.
pub fn cli() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let usage =
        "usage: ci-receipt-metadata module ENV FILE | first-binding ENV DIRECTORY | prefix FILE";
    let output = match args.as_slice() {
        [op, env, file] if op == "module" => {
            module(&read_receipt(Path::new(file))?, env.to_str().ok_or(usage)?)?
        }
        [op, env, dir] if op == "first-binding" => {
            first_binding(Path::new(dir), env.to_str().ok_or(usage)?)?
        }
        [op, file] if op == "prefix" => prefix(&read_receipt(Path::new(file))?)?,
        _ => return Err(usage.into()),
    };
    std::io::stdout().write_all(&output)?;
    Ok(())
}
