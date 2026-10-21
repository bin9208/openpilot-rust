//! Opt-in scanner for the argparse grammar used by native build tools.
use std::collections::BTreeMap;
#[derive(Debug)]
pub struct ArgumentError(pub String);
impl std::fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ArgumentError {}
#[derive(Clone, Copy)]
pub struct OptionSpec {
    pub name: &'static str,
    takes_value: bool,
    required: bool,
    group: Option<&'static str>,
    append: bool,
}
impl OptionSpec {
    pub const fn value(name: &'static str, required: bool) -> Self {
        Self {
            name,
            takes_value: true,
            required,
            group: None,
            append: false,
        }
    }
    pub const fn append(name: &'static str) -> Self {
        Self {
            name,
            takes_value: true,
            required: false,
            group: None,
            append: true,
        }
    }
    pub const fn flag(name: &'static str, group: Option<&'static str>) -> Self {
        Self {
            name,
            takes_value: false,
            required: false,
            group,
            append: false,
        }
    }
}
pub struct Parsed {
    pub values: BTreeMap<String, String>,
    pub help: bool,
    pub positionals: Vec<String>,
    pub append_values: BTreeMap<String, Vec<String>>,
}
mod python_decimal;
pub(crate) fn decimal_value(value: char) -> Option<u8> {
    let point = u32::from(value);
    python_decimal::RANGES.iter().find_map(|&(first, last)| {
        if (first..=last).contains(&point) {
            // Contiguous styled mathematical digit sets repeat 0..9.
            Some(((point - first) % 10) as u8)
        } else {
            None
        }
    })
}
pub(crate) fn decimal(value: &str) -> bool {
    value.chars().all(|ch| decimal_value(ch).is_some())
}

fn negative_number(value: &str) -> bool {
    let value = value.strip_suffix('\n').unwrap_or(value);
    let Some(body) = value.strip_prefix('-') else {
        return false;
    };
    if let Some((before, after)) = body.split_once('.') {
        !after.is_empty() && decimal(before) && decimal(after)
    } else {
        !body.is_empty() && decimal(body)
    }
}
fn option(value: &str) -> bool {
    !value.contains(' ') && value != "-" && value.starts_with('-') && !negative_number(value)
}
pub fn scan(args: &[String], specs: &[OptionSpec]) -> Result<Parsed, ArgumentError> {
    scan_with_positionals(args, specs, 0)
}
/// Opt in to a fixed number of required positional arguments, including `--` literals.
pub fn scan_with_positionals(
    args: &[String],
    specs: &[OptionSpec],
    required_positionals: usize,
) -> Result<Parsed, ArgumentError> {
    scan_with_policy(args, specs, required_positionals, false)
}
fn scan_with_policy(
    args: &[String],
    specs: &[OptionSpec],
    required_positionals: usize,
    required_first: bool,
) -> Result<Parsed, ArgumentError> {
    let mut all = specs.to_vec();
    all.push(OptionSpec::flag("--help", None));
    let resolve = |name: &str| -> Result<Option<usize>, ArgumentError> {
        // Classify short help clusters here; validate explicit tails when the
        // action is consumed so an earlier help action keeps its ordering.
        if name.starts_with("-h") && !name.starts_with("--") {
            return Ok(Some(all.len() - 1));
        }
        if let Some(i) = all.iter().position(|s| s.name == name) {
            return Ok(Some(i));
        }
        let candidates = all
            .iter()
            .enumerate()
            .filter(|(_, s)| name.starts_with("--") && s.name.starts_with(name))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => Ok(None),
            [i] => Ok(Some(*i)),
            _ => Err(ArgumentError(format!("ambiguous option: {name}"))),
        }
    };
    // argparse classifies every option (and rejects ambiguity) before executing help/actions.
    let mut classified = Vec::new();
    let mut terminated = false;
    for arg in args {
        if arg == "--" {
            terminated = true;
            classified.push(None);
            continue;
        }
        classified.push(if terminated {
            None
        } else {
            resolve(arg.split_once('=').map_or(arg.as_str(), |(n, _)| n))?
        });
    }
    let mut values = BTreeMap::new();
    let mut append_values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut groups = BTreeMap::new();
    let mut extras = false;
    let mut positionals = Vec::new();
    let mut literal = false;
    let mut i = 0;
    while i < args.len() {
        let Some(index) = classified[i] else {
            if required_positionals > 0 && args[i] == "--" && !literal {
                literal = true;
            } else if required_positionals > 0 && (literal || !option(&args[i])) {
                positionals.push(args[i].clone());
            } else {
                extras = true;
            }
            i += 1;
            continue;
        };
        let spec = all[index];
        let inline = if args[i].starts_with("-h") && !args[i].starts_with("--") {
            let tail = args[i][2..].trim_start_matches('h');
            if let Some(value) = tail.strip_prefix('=') {
                Some(value)
            } else if tail.starts_with('-') {
                Some(tail)
            } else {
                None
            }
        } else {
            args[i].split_once('=').map(|(_, v)| v)
        };
        if !spec.takes_value {
            if inline.is_some() {
                return Err(ArgumentError(format!(
                    "{} does not accept a value",
                    spec.name
                )));
            }
            if spec.name == "--help" {
                return Ok(Parsed {
                    values,
                    help: true,
                    positionals,
                    append_values,
                });
            }
            if let Some(group) = spec.group {
                if let Some(previous) = groups.insert(group, spec.name) {
                    if previous != spec.name {
                        return Err(ArgumentError(format!(
                            "{} conflicts with {previous}",
                            spec.name
                        )));
                    }
                }
            }
            values.insert(spec.name.to_owned(), String::new());
        } else {
            let value = if let Some(v) = inline {
                v
            } else {
                i += 1;
                args.get(i)
                    .filter(|v| classified[i].is_none() && !option(v))
                    .ok_or_else(|| ArgumentError(format!("{} requires a value", spec.name)))?
            };
            if spec.append {
                append_values
                    .entry(spec.name.to_owned())
                    .or_default()
                    .push(value.to_owned());
            } else {
                values.insert(spec.name.to_owned(), value.to_owned());
            }
        }
        i += 1;
    }
    let unexpected = extras || positionals.len() != required_positionals;
    if required_first || !unexpected {
        for spec in specs {
            if spec.required
                && !values.contains_key(spec.name)
                && !append_values.contains_key(spec.name)
            {
                return Err(ArgumentError(format!("{} required", spec.name)));
            }
        }
    }
    if unexpected {
        return Err(ArgumentError("unrecognized arguments".into()));
    }
    Ok(Parsed {
        values,
        help: false,
        positionals,
        append_values,
    })
}

/// Lossless Unix path arguments with the same grammar as `scan`.
/// Only invalid UTF-8 bytes are encoded for scanning. The marker is chosen not
/// to occur in any original byte string or option name and is reversed in values.
pub struct ParsedOs {
    pub values: BTreeMap<String, std::ffi::OsString>,
    pub help: bool,
    pub positionals: Vec<std::ffi::OsString>,
    pub append_values: BTreeMap<String, Vec<std::ffi::OsString>>,
}
pub fn scan_os(
    args: &[std::ffi::OsString],
    specs: &[OptionSpec],
) -> Result<ParsedOs, ArgumentError> {
    scan_os_with_positionals(args, specs, 0)
}
/// Lossless Unix arguments with a fixed required positional count.
pub fn scan_os_with_positionals(
    args: &[std::ffi::OsString],
    specs: &[OptionSpec],
    required_positionals: usize,
) -> Result<ParsedOs, ArgumentError> {
    scan_os_with_policy(args, specs, required_positionals, false)
}
/// Opt-in to argparse's required-option check before its final unknown-argument
/// check. Existing scanners retain their historical error-order contract.
pub fn scan_os_required_first(
    args: &[std::ffi::OsString],
    specs: &[OptionSpec],
) -> Result<ParsedOs, ArgumentError> {
    scan_os_with_policy(args, specs, 0, true)
}
fn scan_os_with_policy(
    args: &[std::ffi::OsString],
    specs: &[OptionSpec],
    required_positionals: usize,
    required_first: bool,
) -> Result<ParsedOs, ArgumentError> {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
    };
    let marker = (0usize..)
        .map(|n| format!("\u{e000}{n}\u{e001}"))
        .find(|marker| {
            !args.iter().any(|arg| {
                arg.as_bytes()
                    .windows(marker.len())
                    .any(|v| v == marker.as_bytes())
            }) && !specs.iter().any(|spec| spec.name.contains(marker))
        })
        .expect("finite argument strings leave an unused marker");
    let encode = |arg: &OsString| {
        use std::fmt::Write;
        let mut output = String::new();
        let mut rest = arg.as_bytes();
        while !rest.is_empty() {
            match std::str::from_utf8(rest) {
                Ok(text) => {
                    output.push_str(text);
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    output.push_str(std::str::from_utf8(&rest[..valid]).expect("validated prefix"));
                    rest = &rest[valid..];
                    let count = error.error_len().unwrap_or(rest.len());
                    for byte in &rest[..count] {
                        write!(output, "{marker}{byte:02x}").expect("String write");
                    }
                    rest = &rest[count..];
                }
            }
        }
        output
    };
    let decode = |text: &str| {
        let mut output = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find(&marker) {
            output.extend_from_slice(&rest.as_bytes()[..at]);
            rest = &rest[at + marker.len()..];
            output.push(u8::from_str_radix(&rest[..2], 16).expect("encoded raw byte"));
            rest = &rest[2..];
        }
        output.extend_from_slice(rest.as_bytes());
        OsString::from_vec(output)
    };
    let parsed = scan_with_policy(
        &args.iter().map(encode).collect::<Vec<_>>(),
        specs,
        required_positionals,
        required_first,
    )?;
    Ok(ParsedOs {
        values: parsed
            .values
            .into_iter()
            .map(|(key, value)| (key, decode(&value)))
            .collect(),
        help: parsed.help,
        positionals: parsed.positionals.iter().map(|v| decode(v)).collect(),
        append_values: parsed
            .append_values
            .into_iter()
            .map(|(key, values)| (key, values.iter().map(|value| decode(value)).collect()))
            .collect(),
    })
}

#[cfg(test)]
mod decimal_tests {
    #[test]
    fn decimal_values_match_all_independent_python_312_digits() {
        let original: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/panda-cli/python-decimal-values.json"
        ))
        .unwrap();
        let rows = original["values"].as_array().unwrap();
        assert_eq!(rows.len(), 680);
        assert_eq!(original["unicode_version"], "15.0.0");
        for row in rows {
            let ch = char::from_u32(row[0].as_u64().unwrap() as u32).unwrap();
            assert_eq!(
                super::decimal_value(ch),
                Some(row[1].as_u64().unwrap() as u8),
                "U+{:X}",
                u32::from(ch)
            );
        }
        for ch in ['\u{10d40}', '\u{1ccf0}', '²', '①', '-', '.'] {
            assert_eq!(super::decimal_value(ch), None);
        }
    }
}

#[cfg(test)]
mod required_first_tests {
    use super::*;
    use std::ffi::OsString;
    #[test]
    fn required_order_is_opt_in_and_does_not_override_help_or_value_errors() {
        let specs = [OptionSpec::value("--source", true)];
        let unknown = [OsString::from("--unknown")];
        assert_eq!(
            scan_os(&unknown, &specs).err().unwrap().0,
            "unrecognized arguments"
        );
        assert_eq!(
            scan_os_required_first(&unknown, &specs).err().unwrap().0,
            "--source required"
        );
        let complete = ["--source", "value", "--unknown"].map(OsString::from);
        assert_eq!(
            scan_os_required_first(&complete, &specs).err().unwrap().0,
            "unrecognized arguments"
        );
        let help = ["--unknown", "--help"].map(OsString::from);
        assert!(scan_os_required_first(&help, &specs).unwrap().help);
        let missing_value = ["--source", "--unknown"].map(OsString::from);
        assert_eq!(
            scan_os_required_first(&missing_value, &specs)
                .err()
                .unwrap()
                .0,
            "--source requires a value"
        );
    }
}
