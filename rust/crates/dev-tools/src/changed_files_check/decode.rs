//! Source-byte frontend based on CPython3.12.15 tokenizer's BOM/cookie order.
//! Python/PSF source provenance is recorded beside this opt-in candidate.
//! GNU iconv is an explicit native dependency for three legacy codecs and UTF7.
//! Its complete codec-domain parity is NOT yet established by the original72rows.
use super::{named_character, Diagnostic, Phase};
use std::ffi::{c_char, c_void, CStr};

type Result<T> = std::result::Result<T, Diagnostic>;
fn invalid(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Phase::Decode, message)
}

fn normal_name(raw: &[u8]) -> String {
    // CPython's special UTF8/Latin1 normalization examines at most12 bytes.
    let prefix = raw
        .iter()
        .take(12)
        .map(|&b| {
            if b == b'_' {
                b'-'
            } else {
                b.to_ascii_lowercase()
            }
        })
        .collect::<Vec<_>>();
    if prefix == b"utf-8" || prefix.starts_with(b"utf-8-") {
        return "utf-8".into();
    }
    if [b"latin-1".as_slice(), b"iso-8859-1", b"iso-latin-1"]
        .iter()
        .any(|name| {
            prefix == *name
                || prefix
                    .strip_prefix(*name)
                    .is_some_and(|v| v.starts_with(b"-"))
        })
    {
        return "iso-8859-1".into();
    }
    // Cookies admit only ASCII letters, digits, '-', '_', '.'.
    String::from_utf8(raw.to_vec()).expect("ASCII encoding cookie")
}
fn cookie(line: &[u8]) -> Option<String> {
    let first = line.iter().position(|&b| !matches!(b, b' ' | b'\t' | 12))?;
    if line[first] != b'#' {
        return None;
    }
    for i in first..line.len() {
        if !line[i..].starts_with(b"coding") {
            continue;
        }
        let mut p = i + 6;
        if !line.get(p).is_some_and(|b| matches!(b, b':' | b'=')) {
            continue;
        }
        p += 1;
        while line.get(p).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            p += 1;
        }
        let start = p;
        while line
            .get(p)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            p += 1;
        }
        if p > start {
            return Some(normal_name(&line[start..p]));
        }
    }
    None
}
fn encoding(raw: &[u8], bom: bool) -> Result<String> {
    let mut selected = None;
    for line in raw.split(|&b| b == b'\n').take(2) {
        if let Some(name) = cookie(line) {
            selected = Some(name);
            break;
        }
        if line
            .iter()
            .copied()
            .find(|b| !matches!(b, b' ' | b'\t' | 12))
            .is_some_and(|b| b != b'#')
        {
            break;
        }
    }
    if bom && selected.as_ref().is_some_and(|name| name != "utf-8") {
        return Err(invalid("encoding cookie conflicts with UTF8 BOM"));
    }
    Ok(selected.unwrap_or_else(|| "utf-8".into()))
}
fn newlines(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 1);
    let mut i = 0;
    while let Some(&b) = raw.get(i) {
        if b == b'\r' {
            out.push(b'\n');
            if raw.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
        } else {
            out.push(b);
        }
        i += 1;
    }
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    out
}
fn scalar(value: u32) -> Result<char> {
    char::from_u32(value)
        .ok_or_else(|| invalid("source codec produced a surrogate or out-of-range code point"))
}
fn hex(raw: &[u8], i: &mut usize, count: usize) -> Result<char> {
    let end = i
        .checked_add(count)
        .ok_or_else(|| invalid("source escape overflow"))?;
    let bytes = raw
        .get(*i..end)
        .ok_or_else(|| invalid("truncated source Unicode escape"))?;
    let mut value = 0u32;
    for &b in bytes {
        let digit = char::from(b)
            .to_digit(16)
            .ok_or_else(|| invalid("invalid source Unicode escape"))?;
        value = value * 16 + digit;
    }
    *i = end;
    scalar(value)
}
fn escaped(raw: &[u8], raw_unicode: bool) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    while let Some(&b) = raw.get(i) {
        i += 1;
        if b != b'\\' {
            out.push(char::from(b));
            continue;
        }
        if raw_unicode {
            let start = i - 1;
            while raw.get(i) == Some(&b'\\') {
                i += 1;
            }
            let count = i - start;
            if count % 2 == 0 || !raw.get(i).is_some_and(|b| matches!(b, b'u' | b'U')) {
                out.extend(std::iter::repeat_n('\\', count));
                continue;
            }
            out.extend(std::iter::repeat_n('\\', count - 1));
        }
        let next = *raw
            .get(i)
            .ok_or_else(|| invalid("trailing source escape"))?;
        i += 1;
        match next {
            b'u' => out.push(hex(raw, &mut i, 4)?),
            b'U' => out.push(hex(raw, &mut i, 8)?),
            _ if raw_unicode => unreachable!("raw Unicode branch only interprets u/U"),
            b'x' => out.push(hex(raw, &mut i, 2)?),
            b'N' => {
                if raw.get(i) != Some(&b'{') {
                    return Err(invalid("malformed source named escape"));
                }
                i += 1;
                let end = raw[i..]
                    .iter()
                    .position(|&b| b == b'}')
                    .map(|p| i + p)
                    .ok_or_else(|| invalid("unterminated source named escape"))?;
                let name = std::str::from_utf8(&raw[i..end]).map_err(|e| invalid(e.to_string()))?;
                out.push(named_character(name)?);
                i = end + 1;
            }
            b'\n' => {}
            b'\\' => out.push('\\'),
            b'\'' => out.push('\''),
            b'"' => out.push('"'),
            b'a' => out.push('\x07'),
            b'b' => out.push('\x08'),
            b'f' => out.push('\x0c'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'v' => out.push('\x0b'),
            b'0'..=b'7' => {
                let mut value = u32::from(next - b'0');
                for _ in 0..2 {
                    if let Some(&digit @ b'0'..=b'7') = raw.get(i) {
                        value = value * 8 + u32::from(digit - b'0');
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(scalar(value)?);
            }
            _ => {
                out.push('\\');
                out.push(char::from(next));
            }
        }
    }
    Ok(out)
}

// Explicit Linux libc codec boundary, no Python interpreter or executable helper.
// Unsafe operations are restricted to GNU iconv's documented pointer API. The
// immutable input is never written; only its cursor and initialized output move.
#[allow(unsafe_code)] // GNU libc's iconv ABI; calls are confined to native_codec/Drop.
unsafe extern "C" {
    fn iconv_open(to: *const c_char, from: *const c_char) -> *mut c_void;
    fn iconv(
        cd: *mut c_void,
        input: *mut *mut c_char,
        input_left: *mut usize,
        output: *mut *mut c_char,
        output_left: *mut usize,
    ) -> usize;
    fn iconv_close(cd: *mut c_void) -> i32;
}
struct Codec(*mut c_void);
impl Drop for Codec {
    #[allow(unsafe_code)] // Owns the handle returned and checked by native_codec.
    fn drop(&mut self) {
        // SAFETY: Every constructed Codec owns one successful iconv_open handle.
        unsafe {
            iconv_close(self.0);
        }
    }
}
#[allow(unsafe_code)] // Length-bounded iconv FFI; each call documents buffer validity.
fn native_codec(raw: &[u8], name: &CStr) -> Result<String> {
    // SAFETY: Static nul-terminated names; the result is checked before use.
    let handle = unsafe { iconv_open(c"UTF-8".as_ptr(), name.as_ptr()) };
    if handle as usize == usize::MAX {
        return Err(Diagnostic::new(
            Phase::Capability,
            format!("native codec {} unavailable", name.to_string_lossy()),
        ));
    }
    let codec = Codec(handle);
    let length = raw
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(32))
        .ok_or_else(|| invalid("source expansion overflow"))?;
    let mut out = vec![0u8; length];
    let mut input = raw.as_ptr().cast::<c_char>().cast_mut();
    let mut output = out.as_mut_ptr().cast::<c_char>();
    let mut input_left = raw.len();
    let mut output_left = out.len();
    // SAFETY: Both cursor pairs point to live buffers with their exact lengths;
    // iconv may read input and write output only within the remaining lengths.
    let status = unsafe {
        iconv(
            codec.0,
            &mut input,
            &mut input_left,
            &mut output,
            &mut output_left,
        )
    };
    if status != 0 || input_left != 0 {
        return Err(invalid(format!(
            "strict native source decoding failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: Null input flushes this live codec state into remaining output.
    let flushed = unsafe {
        iconv(
            codec.0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut output,
            &mut output_left,
        )
    };
    if flushed != 0 {
        return Err(invalid("source codec final state rejected"));
    }
    out.truncate(length - output_left);
    String::from_utf8(out).map_err(|e| invalid(e.to_string()))
}
fn cp1252(raw: &[u8]) -> Result<String> {
    let specials = [
        Some('€'),
        None,
        Some('‚'),
        Some('ƒ'),
        Some('„'),
        Some('…'),
        Some('†'),
        Some('‡'),
        Some('ˆ'),
        Some('‰'),
        Some('Š'),
        Some('‹'),
        Some('Œ'),
        None,
        Some('Ž'),
        None,
        None,
        Some('‘'),
        Some('’'),
        Some('“'),
        Some('”'),
        Some('•'),
        Some('–'),
        Some('—'),
        Some('˜'),
        Some('™'),
        Some('š'),
        Some('›'),
        Some('œ'),
        None,
        Some('ž'),
        Some('Ÿ'),
    ];
    raw.iter()
        .map(|&b| {
            if (0x80..0xa0).contains(&b) {
                specials[usize::from(b - 0x80)]
                    .ok_or_else(|| invalid("undefined cp1252 source byte"))
            } else {
                Ok(char::from(b))
            }
        })
        .collect()
}
pub fn source(raw: &[u8]) -> Result<String> {
    let raw = newlines(raw);
    let bom = raw.starts_with(&[0xef, 0xbb, 0xbf]);
    let raw = if bom { &raw[3..] } else { raw.as_slice() };
    let codec = encoding(raw, bom)?;
    // Python codec lookup is broader than this reviewed candidate roster. A
    // capability error is explicit and never counted as Python syntax rejection.
    let mut key = String::new();
    let mut separator = false;
    for c in codec.chars() {
        if c.is_ascii_alphanumeric() {
            if separator && !key.is_empty() {
                key.push('_');
            }
            separator = false;
            key.push(c.to_ascii_lowercase());
        } else {
            separator = true;
        }
    }
    let name = super::codec_names::lookup(&key)
        .ok_or_else(|| invalid(format!("unknown Python3.12 source encoding: {codec}")))?;
    let decoded = match name {
        "utf_8" | "utf8" | "utf_8_sig" => std::str::from_utf8(raw)
            .map(str::to_owned)
            .map_err(|e| invalid(e.to_string()))?,
        "ascii" | "us_ascii" => {
            if !raw.is_ascii() {
                return Err(invalid("non-ASCII source byte"));
            }
            String::from_utf8(raw.to_vec()).expect("ASCII source")
        }
        "iso_8859_1" | "iso8859_1" | "latin1" | "latin_1" => {
            raw.iter().map(|&b| char::from(b)).collect()
        }
        "cp1252" | "windows_1252" => cp1252(raw)?,
        "unicode_escape" => escaped(raw, false)?,
        "raw_unicode_escape" => escaped(raw, true)?,
        "utf7" | "utf_7" => native_codec(raw, c"UTF-7")?,
        "shift_jis" | "shiftjis" | "sjis" => native_codec(raw, c"SHIFT_JIS")?,
        "cp932" | "ms932" => native_codec(raw, c"CP932")?,
        "koi8_r" | "koi8r" => native_codec(raw, c"KOI8-R")?,
        "base64_codec" | "bz2_codec" | "hex_codec" | "quopri_codec" | "rot_13" | "uu_codec"
        | "zlib_codec" => {
            return Err(invalid(
                "registered byte transform is not a Python source text codec",
            ))
        }
        _ => {
            return Err(Diagnostic::new(
                Phase::Capability,
                format!("source codec {codec:?} is not yet supported by the native candidate"),
            ))
        }
    };
    if decoded.contains('\0') {
        return Err(invalid("source code cannot contain null bytes"));
    }
    Ok(decoded)
}
