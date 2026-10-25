use super::{Command, Options};
use crate::{argparse_scan::ArgumentError, path_resolve::resolve_non_strict, seal::Result};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
const USAGE: &str = "usage: check-navd-config [-h] --binary BINARY --output OUTPUT";
pub(super) const HELP:&str="usage: check-navd-config [-h] --binary BINARY --output OUTPUT\n\noptions:\n  -h, --help       show this help message and exit\n  --binary BINARY\n  --output OUTPUT\n";
fn display(s: &OsStr) -> String {
    crate::python_fs::codepoints(s)
        .into_iter()
        .map(|p| char::from_u32(p).map_or_else(|| format!("\\u{p:04x}"), |c| c.to_string()))
        .collect()
}
fn error(message: impl std::fmt::Display) -> Box<dyn std::error::Error> {
    Box::new(ArgumentError(format!(
        "{USAGE}\ncheck-navd-config: error: {message}"
    )))
}
fn option(s: &OsStr) -> bool {
    let bytes = s.as_bytes();
    if bytes == b"-" || !bytes.starts_with(b"-") || bytes.contains(&b' ') {
        return false;
    }
    let text = display(s);
    let body = text[1..].strip_suffix('\n').unwrap_or(&text[1..]);
    let digits = |s: &str| {
        s.chars()
            .all(|c| crate::argparse_scan::decimal_value(c).is_some())
    };
    !(if let Some((a, b)) = body.split_once('.') {
        digits(a) && !b.is_empty() && digits(b)
    } else {
        !body.is_empty() && digits(body)
    })
}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: BTreeMap<OsString, OsString>,
) -> Result<Command> {
    let names = [b"--help".as_slice(), b"--binary", b"--output"];
    let mut values: [Option<OsString>; 2] = [None, None];
    let mut extras = Vec::new();
    let mut literal = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let raw = arg.as_bytes();
        if raw == b"--" {
            literal = true;
            i += 1;
            continue;
        }
        if !literal && option(arg) {
            let name = raw.split(|b| *b == b'=').next().unwrap_or(raw);
            let help_short = name.starts_with(b"-h") && !name.starts_with(b"--");
            let matches = if help_short {
                vec![0]
            } else {
                names
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| {
                        *n == &name || (name.starts_with(b"--") && n.starts_with(name))
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>()
            };
            if matches.len() > 1 {
                return Err(error(format!("ambiguous option: {}", display(arg))));
            }
            if let Some(&which) = matches.first() {
                let inline = if help_short && !raw.starts_with(b"-h=") {
                    None
                } else {
                    raw.iter()
                        .position(|b| *b == b'=')
                        .map(|at| OsString::from_vec(raw[at + 1..].to_vec()))
                };
                if which == 0 {
                    if let Some(value) = inline {
                        return Err(error(format!(
                            "argument -h/--help: ignored explicit argument '{}'",
                            display(&value)
                        )));
                    }
                    return Ok(Command::Help);
                }
                let value = if let Some(value) = inline {
                    value
                } else {
                    i += 1;
                    args.get(i).filter(|v| !option(v)).cloned().ok_or_else(|| {
                        error(format!(
                            "argument {}: expected one argument",
                            String::from_utf8_lossy(names[which])
                        ))
                    })?
                };
                values[which - 1] = Some(value);
            } else {
                extras.push(arg.clone())
            }
        } else {
            extras.push(arg.clone())
        }
        i += 1;
    }
    let missing = values
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_none())
        .map(|(i, _)| String::from_utf8_lossy(names[i + 1]))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(error(format!(
            "the following arguments are required: {}",
            missing.join(", ")
        )));
    }
    if !extras.is_empty() {
        return Err(error(format!(
            "unrecognized arguments: {}",
            extras
                .iter()
                .map(|s| display(s))
                .collect::<Vec<_>>()
                .join(" ")
        )));
    }
    let resolve = |v: &OsStr| {
        resolve_non_strict(&if Path::new(v).is_absolute() {
            PathBuf::from(v)
        } else {
            cwd.join(v)
        })
    };
    let root = environment
        .get(OsStr::new("OPENPILOT_RUST_ROOT"))
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        );
    Ok(Command::Run(Options {
        root,
        binary: resolve(values[0].as_deref().ok_or("missing binary")?)?,
        output: resolve(values[1].as_deref().ok_or("missing output")?)?,
        environment,
    }))
}
