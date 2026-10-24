//! Source-derived camera constants, preserving original output and failure order.
use crate::{
    argparse_scan::{decimal_value, scan_os, OptionSpec},
    seal::Result,
};
use num_bigint::BigInt;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

#[path = "camera_data_unicode.rs"]
pub mod unicode;

fn python_space(ch: char) -> bool {
    ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch)
}

fn integer(value: &str, auto_base: bool) -> Result<BigInt> {
    let value = value.trim_matches(char::is_whitespace);
    let normalized: String = value
        .chars()
        .map(|ch| decimal_value(ch).map_or(ch, |digit| char::from(b'0' + digit)))
        .collect();
    let (negative, mut body) = if let Some(body) = normalized.strip_prefix('-') {
        (true, body)
    } else {
        (false, normalized.strip_prefix('+').unwrap_or(&normalized))
    };
    let mut base = 10;
    let mut prefixed = false;
    if auto_base && body.len() >= 2 && body.starts_with('0') {
        base = match body.as_bytes()[1] {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => 10,
        };
        if base != 10 {
            prefixed = true;
            body = &body[2..];
        }
    }
    let mut digits = String::new();
    let mut last_digit = false;
    for (index, ch) in body.chars().enumerate() {
        if ch == '_' {
            if !(last_digit || prefixed && index == 0) {
                return Err("invalid integer separator".into());
            }
            last_digit = false;
        } else {
            if !ch.is_ascii() || ch.to_digit(base).is_none() {
                return Err("invalid integer digit".into());
            }
            digits.push(ch);
            last_digit = true;
        }
    }
    if !last_digit || (base == 10 && digits.len() > 4300) {
        return Err("invalid or overlong integer".into());
    }
    if auto_base && base == 10 && digits.starts_with('0') && digits.bytes().any(|b| b != b'0') {
        return Err("invalid leading zero".into());
    }
    let result = BigInt::parse_bytes(digits.as_bytes(), base).ok_or("invalid integer")?;
    Ok(if negative { -result } else { result })
}

pub fn bps(root: &Path, output: &mut dyn Write) -> Result<()> {
    let path = root.join("openpilot/system/camerad/cameras/bps_blobs.h");
    let source = fs::read_to_string(&path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    writeln!(
        output,
        "// Generated from openpilot/system/camerad/cameras/bps_blobs.h; original project MIT license applies."
    )?;
    writeln!(
        output,
        "// Source SHA-256: {:x}",
        Sha256::digest(fs::read(path)?)
    )?;
    writeln!(
        output,
        "// Regenerate with rust/tools/generate_camerad_bps_data.py --source <checkout>.\n"
    )?;
    // Both classes are captured from the original Unicode15 interpreter; do
    // not let the regex dependency's newer Unicode tables change extraction.
    let arrays = Regex::new(&unicode::array_pattern())?;
    let arrays = arrays.captures_iter(&source).collect::<Vec<_>>();
    if arrays.len() != 3 {
        return Err("BPS source must contain three arrays".into());
    }
    let rows = Regex::new(r"\{([^{}]*)\}")?;
    let comments = Regex::new(r"(?s)/\*.*?\*/")?;
    for array in arrays {
        let count = integer(&array[2], false)?;
        let width = integer(&array[3], true)?;
        let rows = rows.captures_iter(&array[4]).collect::<Vec<_>>();
        if BigInt::from(rows.len()) != count {
            return Err("BPS row count differs".into());
        }
        let width_text = width.to_string();
        if width_text.trim_start_matches('-').len() > 4300 {
            return Err("integer representation exceeds Python limit".into());
        }
        writeln!(
            output,
            "pub const {}: [[u8; {width_text}]; {count}] = [",
            unicode::uppercase(&array[1])
        )?;
        for row in rows {
            let clean = comments.replace_all(&row[1], "");
            let values = clean
                .split(',')
                .map(|v| v.trim_matches(python_space))
                .filter(|v| !v.is_empty())
                .map(|v| integer(v, true))
                .collect::<Result<Vec<_>>>()?;
            if BigInt::from(values.len()) > width
                || values
                    .iter()
                    .any(|v| v < &BigInt::from(0) || v > &BigInt::from(255))
            {
                return Err("invalid BPS byte row".into());
            }
            let width: usize = width_text.parse()?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(width)?;
            for value in values {
                bytes.push(value.to_string().parse::<u8>()?);
            }
            bytes.resize(width, 0);
            if bytes.iter().all(|v| *v == 0) {
                writeln!(output, "    [0; {width}],")?;
                continue;
            }
            writeln!(output, "    [")?;
            for row in bytes.chunks(15) {
                write!(output, "        ")?;
                for (index, value) in row.iter().enumerate() {
                    if index > 0 {
                        write!(output, ", ")?;
                    }
                    write!(output, "0x{value:02x}")?;
                }
                writeln!(output, ",")?;
            }
            writeln!(output, "    ],")?;
        }
        writeln!(output, "];\n")?;
    }
    Ok(())
}

pub fn bps_cli() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = match scan_os(&args, &[OptionSpec::value("--source", true)]) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if parsed.help {
        println!("Usage: generate-camerad-bps-data --source PATH");
        return;
    }
    if let Err(error) = bps(
        Path::new(&parsed.values["--source"]),
        &mut std::io::stdout().lock(),
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
