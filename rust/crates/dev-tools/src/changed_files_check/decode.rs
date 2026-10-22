//! Source-byte frontend based on CPython3.12.15 tokenizer's BOM/cookie order.
//! Python/PSF source provenance is recorded beside this opt-in candidate.
//! GNU iconv is an explicit native dependency for the remaining multibyte codecs.
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
fn native_codec(raw: &[u8], name: &CStr) -> Result<String> {
    // SAFETY: Static nul-terminated names; the result is checked before use.
    #[allow(unsafe_code)] // Confined to this checked GNU libc FFI call.
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
    #[allow(unsafe_code)] // Confined to this bounded GNU libc FFI call.
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
    #[allow(unsafe_code)] // Confined to this bounded GNU libc FFI call.
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
fn utf16(raw: &[u8], endian: Option<bool>) -> Result<String> {
    let (raw, little) = match (endian, raw.get(..2)) {
        (None, Some([0xff, 0xfe])) => (&raw[2..], true),
        (None, Some([0xfe, 0xff])) => (&raw[2..], false),
        _ => (raw, endian.unwrap_or(cfg!(target_endian = "little"))),
    };
    if !raw.len().is_multiple_of(2) {
        return Err(invalid("truncated UTF16 payload"));
    }
    let units = raw.chunks_exact(2).map(|v| {
        let bytes = [v[0], v[1]];
        if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        }
    });
    char::decode_utf16(units)
        .map(|c| c.map_err(|_| invalid("unpaired UTF16 surrogate")))
        .collect()
}
fn utf32(raw: &[u8], endian: Option<bool>) -> Result<String> {
    let (raw, little) = match (endian, raw.get(..4)) {
        (None, Some([0xff, 0xfe, 0, 0])) => (&raw[4..], true),
        (None, Some([0, 0, 0xfe, 0xff])) => (&raw[4..], false),
        _ => (raw, endian.unwrap_or(cfg!(target_endian = "little"))),
    };
    if !raw.len().is_multiple_of(4) {
        return Err(invalid("truncated UTF32 payload"));
    }
    raw.chunks_exact(4)
        .map(|v| {
            let bytes = [v[0], v[1], v[2], v[3]];
            scalar(if little {
                u32::from_le_bytes(bytes)
            } else {
                u32::from_be_bytes(bytes)
            })
        })
        .collect()
}
/// Strict payload decoding without source BOM/cookie/newline/NUL admission.
/// Unlike `source`, this preserves NUL, CR, LF and missing EOF newlines.
pub(super) fn payload(raw: &[u8], codec: &str) -> Result<String> {
    // Python codec lookup is broader than this reviewed candidate roster. A
    // capability error is explicit and never counted as Python syntax rejection.
    let mut key = String::new();
    let mut separator = false;
    for c in codec.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
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
    if let Some(table) = super::single_byte_tables::lookup(name) {
        return raw
            .iter()
            .map(|&b| {
                let cp = table[usize::from(b)];
                if cp == 0xfffe {
                    Err(invalid("undefined byte in pinned CPython codec table"))
                } else {
                    scalar(cp)
                }
            })
            .collect();
    }
    let decoded = match name {
        "utf_8" | "utf8" => std::str::from_utf8(raw)
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
        "utf_8_sig" => std::str::from_utf8(raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw))
            .map(str::to_owned)
            .map_err(|e| invalid(e.to_string()))?,
        "charmap" => raw.iter().map(|&b| char::from(b)).collect(),
        "utf_16" => utf16(raw, None)?,
        "utf_16_le" => utf16(raw, Some(true))?,
        "utf_16_be" => utf16(raw, Some(false))?,
        "utf_32" => utf32(raw, None)?,
        "utf_32_le" => utf32(raw, Some(true))?,
        "utf_32_be" => utf32(raw, Some(false))?,
        // These registry modules have no codec on the supported Linux host.
        "mbcs" | "oem" | "undefined" => {
            return Err(invalid("source text codec unavailable on Linux"))
        }
        "punycode" => super::punycode::decode(raw)?,
        "idna" => super::idna::decode(raw)?,
        "unicode_escape" => escaped(raw, false)?,
        "raw_unicode_escape" => escaped(raw, true)?,
        "utf7" | "utf_7" => super::utf7::decode(raw)?,
        "shift_jis" | "shiftjis" | "sjis" => super::japanese::decode(raw, false)?,
        "cp932" | "ms932" => super::japanese::decode(raw, true)?,
        "koi8_r" | "koi8r" => native_codec(raw, c"KOI8-R")?,
        "euc_jp" | "euc_jis_2004" | "euc_jisx0213" => super::japanese_variants::euc(raw, name)?,
        "shift_jis_2004" | "shift_jisx0213" => {
            super::japanese_variants::shift(raw, name == "shift_jisx0213")?
        }
        "iso2022_jp" | "iso2022_jp_1" | "iso2022_jp_3" | "iso2022_jp_2004" | "iso2022_jp_ext" => {
            super::japanese_variants::iso2022(raw, name)?
        }
        "hz" => super::hz::decode(raw)?,
        "big5" => native_codec(raw, c"BIG5")?,
        "big5hkscs" => native_codec(raw, c"BIG5-HKSCS")?,
        "cp949" => native_codec(raw, c"CP949")?,
        "cp950" => native_codec(raw, c"CP950")?,
        "euc_kr" => super::korean::euc(raw)?,
        "gb18030" => native_codec(raw, c"GB18030")?,
        "gb2312" => native_codec(raw, c"GB2312")?,
        "gbk" => native_codec(raw, c"GBK")?,
        "iso2022_jp_2" => native_codec(raw, c"ISO-2022-JP-2")?,
        "iso2022_kr" => super::korean::iso2022(raw)?,
        "johab" => super::korean::johab(raw)?,
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
    Ok(decoded)
}
pub fn source(raw: &[u8]) -> Result<String> {
    // CPython _Py_SourceAsString checks raw embedded NUL before token decoding.
    if raw.contains(&0) {
        return Err(invalid("source code string cannot contain null bytes"));
    }
    let raw = newlines(raw);
    let bom = raw.starts_with(&[0xef, 0xbb, 0xbf]);
    let raw = if bom { &raw[3..] } else { raw.as_slice() };
    let codec = encoding(raw, bom)?;
    let decoded = payload(raw, &codec)?;
    if decoded.contains('\0') {
        return Err(invalid("source code cannot contain null bytes"));
    }
    Ok(decoded)
}
