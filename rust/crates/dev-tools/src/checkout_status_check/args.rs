//! Source-compatible argparse admission; paths retain native Unix bytes.
use super::{ArgumentError, Result};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
};
const OPTIONS: [&str; 8] = [
    "--binary",
    "--fixture",
    "--launcher",
    "--captured-binary",
    "--handshake-fixture",
    "--output",
    "--group",
    "--help",
];
// Retain the original diagnostic label and wrapping as a CLI compatibility
// contract. The executable and every child are native ELF programs.
pub const USAGE:&str="usage: check_checkout_status.py [-h] --binary BINARY --fixture FIXTURE\n                                --launcher LAUNCHER\n                                [--captured-binary CAPTURED_BINARY]\n                                [--handshake-fixture HANDSHAKE_FIXTURE]\n                                --output OUTPUT\n                                [--group {all,metadata,git,status,helper}]\n";
pub fn error(detail: impl AsRef<str>) -> ArgumentError {
    ArgumentError(format!(
        "{USAGE}check_checkout_status.py: error: {}",
        detail.as_ref()
    ))
}
fn option(value: &OsStr) -> bool {
    let bytes = value.as_encoded_bytes();
    if bytes == b"-" || !bytes.starts_with(b"-") || bytes.contains(&b' ') {
        return false;
    }
    let number = value.to_str().and_then(|s| s.strip_prefix('-'));
    let negative = number.is_some_and(|s| {
        let s = s.strip_suffix('\n').unwrap_or(s);
        if let Some((before, after)) = s.split_once('.') {
            !after.is_empty()
                && before
                    .chars()
                    .all(|c| crate::argparse_scan::decimal_value(c).is_some())
                && after
                    .chars()
                    .all(|c| crate::argparse_scan::decimal_value(c).is_some())
        } else {
            !s.is_empty()
                && s.chars()
                    .all(|c| crate::argparse_scan::decimal_value(c).is_some())
        }
    });
    !negative
}
fn resolve(value: &OsStr) -> Result<Option<&'static str>> {
    let Some(text) = value.to_str() else {
        return Ok(None);
    };
    let name = text.split_once('=').map_or(text, |(name, _)| name);
    if name.starts_with("-h") && !name.starts_with("--") {
        return Ok(Some("--help"));
    }
    if let Some(exact) = OPTIONS.iter().find(|v| **v == name) {
        return Ok(Some(*exact));
    }
    let candidates = OPTIONS
        .iter()
        .copied()
        .filter(|v| name.starts_with("--") && v.starts_with(name))
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(*value)),
        _ => Err(error(format!(
            "ambiguous option: {name} could match {}",
            candidates.join(", ")
        ))
        .into()),
    }
}
pub fn parse(args: &[OsString]) -> Result<Option<BTreeMap<String, OsString>>> {
    let mut classification = Vec::new();
    let mut literal = false;
    for value in args {
        if value == "--" {
            literal = true;
            classification.push(None);
        } else {
            classification.push(if literal { None } else { resolve(value)? });
        }
    }
    let mut values = BTreeMap::new();
    let mut extras = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let Some(name) = classification[i] else {
            extras.push(args[i].clone());
            i += 1;
            continue;
        };
        let bytes = args[i].as_encoded_bytes();
        let inline = bytes.iter().position(|b| *b == b'=').map(|at| {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(bytes[at + 1..].to_vec())
        });
        if name == "--help" {
            if let Some(value) = inline {
                return Err(error(format!(
                    "argument -h/--help: ignored explicit argument {}",
                    crate::plot_samples_source::text_repr(&crate::python_fs::codepoints(&value))
                ))
                .into());
            }
            return Ok(None);
        }
        let value = if let Some(value) = inline {
            value
        } else {
            i += 1;
            args.get(i)
                .filter(|v| classification[i].is_none() && !option(v))
                .cloned()
                .ok_or_else(|| error(format!("argument {name}: expected one argument")))?
        };
        if name == "--group"
            && !["all", "metadata", "git", "status", "helper"]
                .iter()
                .any(|v| value == OsStr::new(v))
        {
            return Err(error(format!("argument --group: invalid choice: {} (choose from 'all', 'metadata', 'git', 'status', 'helper')",crate::plot_samples_source::text_repr(&crate::python_fs::codepoints(&value)))).into());
        }
        values.insert(name.to_owned(), value);
        i += 1;
    }
    let missing = ["--binary", "--fixture", "--launcher", "--output"]
        .into_iter()
        .filter(|k| !values.contains_key(*k))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(error(format!(
            "the following arguments are required: {}",
            missing.join(", ")
        ))
        .into());
    }
    if !extras.is_empty() {
        return Err(error(format!(
            "unrecognized arguments: {}",
            extras
                .iter()
                .map(|v| v.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        ))
        .into());
    }
    Ok(Some(values))
}
pub fn help() -> String {
    format!("{USAGE}\noptions:\n  -h, --help            show this help message and exit\n  --binary BINARY\n  --fixture FIXTURE\n  --launcher LAUNCHER\n  --captured-binary CAPTURED_BINARY\n  --handshake-fixture HANDSHAKE_FIXTURE\n  --output OUTPUT\n  --group {{all,metadata,git,status,helper}}\n")
}
