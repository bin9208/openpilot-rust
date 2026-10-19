use super::{Command, Options, Suite};
use crate::{
    argparse_scan::ArgumentError, path_resolve::resolve_non_strict, python_fs, seal::Result,
};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
const USAGE:&str="usage: check-registration [-h] [--runner RUNNER] [--case CASE] [--timeouts]\n                          [--hardware]\n                          binary output";
pub(super) const HELP:&str="usage: check-registration [-h] [--runner RUNNER] [--case CASE] [--timeouts]\n                          [--hardware]\n                          binary output\n\npositional arguments:\n  binary\n  output\n\noptions:\n  -h, --help       show this help message and exit\n  --runner RUNNER\n  --case CASE\n  --timeouts\n  --hardware\n";
fn error(message: impl std::fmt::Display) -> Box<dyn std::error::Error> {
    Box::new(ArgumentError(format!(
        "{USAGE}\ncheck-registration: error: {message}"
    )))
}
fn display(value: &OsStr) -> String {
    python_fs::codepoints(value)
        .into_iter()
        .map(|p| char::from_u32(p).map_or_else(|| format!("\\u{p:04x}"), |c| c.to_string()))
        .collect()
}
fn option(value: &OsStr) -> bool {
    let bytes = value.as_bytes();
    if bytes == b"-" || !bytes.starts_with(b"-") || bytes.contains(&b' ') {
        return false;
    }
    let text = display(value);
    let body = text[1..].strip_suffix('\n').unwrap_or(&text[1..]);
    let decimal = |s: &str| {
        s.chars()
            .all(|c| crate::argparse_scan::decimal_value(c).is_some())
    };
    !(if let Some((a, b)) = body.split_once('.') {
        decimal(a) && !b.is_empty() && decimal(b)
    } else {
        !body.is_empty() && decimal(body)
    })
}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: BTreeMap<OsString, OsString>,
) -> Result<Command> {
    const NAMES: [&[u8]; 5] = [
        b"--help",
        b"--runner",
        b"--case",
        b"--timeouts",
        b"--hardware",
    ];
    let resolve = |arg: &OsStr| -> Result<Option<usize>> {
        let raw = arg.as_bytes();
        let name = raw.split(|b| *b == b'=').next().unwrap_or(raw);
        if name.starts_with(b"-h") && !name.starts_with(b"--") {
            return Ok(Some(0));
        }
        if let Some(i) = NAMES.iter().position(|x| *x == name) {
            return Ok(Some(i));
        }
        let candidates = NAMES
            .iter()
            .enumerate()
            .filter(|(_, x)| name.starts_with(b"--") && x.starts_with(name))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => Ok(None),
            [i] => Ok(Some(*i)),
            _ => Err(error(format!(
                "ambiguous option: {} could match {}",
                display(arg),
                candidates
                    .iter()
                    .map(|i| String::from_utf8_lossy(NAMES[*i]))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    };
    let mut classified = Vec::new();
    let mut literal = false;
    for arg in args {
        if arg == "--" {
            literal = true;
            classified.push(None)
        } else {
            classified.push(if literal { None } else { resolve(arg)? })
        }
    }
    let mut positions = Vec::new();
    let mut extras = Vec::new();
    let mut runner = Vec::new();
    let mut case = None;
    let mut hardware = false;
    let mut timeouts = false;
    literal = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !literal && arg == "--" {
            literal = true;
            i += 1;
            continue;
        }
        if !literal && option(arg) {
            if let Some(which) = classified[i] {
                let raw = arg.as_bytes();
                let inline = if which == 0 && raw.starts_with(b"-h") && !raw.starts_with(b"-h=") {
                    None
                } else {
                    raw.iter()
                        .position(|b| *b == b'=')
                        .map(|at| OsString::from_vec(raw[at + 1..].to_vec()))
                };
                if which == 0 || which >= 3 {
                    if let Some(value) = inline {
                        return Err(error(format!(
                            "argument {}: ignored explicit argument '{}'",
                            String::from_utf8_lossy(NAMES[which]),
                            display(&value)
                        )));
                    }
                    if which == 0 {
                        return Ok(Command::Help);
                    }
                    if which == 3 {
                        timeouts = true
                    } else {
                        hardware = true
                    }
                } else {
                    let value = if let Some(value) = inline {
                        value
                    } else {
                        i += 1;
                        args.get(i).filter(|v| !option(v)).cloned().ok_or_else(|| {
                            error(format!(
                                "argument {}: expected one argument",
                                String::from_utf8_lossy(NAMES[which])
                            ))
                        })?
                    };
                    if which == 1 {
                        runner.push(value)
                    } else {
                        case = Some(display(&value))
                    }
                }
            } else {
                extras.push(arg.clone())
            }
        } else if positions.len() < 2 {
            positions.push(arg.clone())
        } else {
            extras.push(arg.clone())
        }
        i += 1;
    }
    if positions.len() < 2 {
        return Err(error(if positions.is_empty() {
            "the following arguments are required: binary, output"
        } else {
            "the following arguments are required: output"
        }));
    }
    if !extras.is_empty() {
        return Err(error(format!(
            "unrecognized arguments: {}",
            extras
                .iter()
                .map(|v| display(v))
                .collect::<Vec<_>>()
                .join(" ")
        )));
    }
    let path = |v: &OsStr| {
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
        binary: path(&positions[0])?,
        output: path(&positions[1])?,
        runner,
        case,
        suite: if timeouts {
            Suite::Timeouts
        } else if hardware {
            Suite::Hardware
        } else {
            Suite::All
        },
        environment,
    }))
}
