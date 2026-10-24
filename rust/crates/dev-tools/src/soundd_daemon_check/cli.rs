use super::{require, Mode};
use crate::{argparse_scan::ArgumentError, path_resolve::resolve_non_strict, seal::Result};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub fixture_assets_binary: PathBuf,
    pub portaudio_library: Option<PathBuf>,
    pub modes: Vec<Mode>,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}
const CHOICES: &str = "live, bounded, retry, inactive, start-failed, stop-retry, exhausted";
const USAGE:&str="usage: check-soundd-daemon [-h] --binary BINARY --output OUTPUT\n                              --fixture-assets-binary FIXTURE_ASSETS_BINARY\n                              [--portaudio-library PORTAUDIO_LIBRARY]\n                              [--mode {live,bounded,retry,inactive,start-failed,stop-retry,exhausted}]";
pub(super) const HELP:&str="usage: check-soundd-daemon [-h] --binary BINARY --output OUTPUT\n                              --fixture-assets-binary FIXTURE_ASSETS_BINARY\n                              [--portaudio-library PORTAUDIO_LIBRARY]\n                              [--mode {live,bounded,retry,inactive,start-failed,stop-retry,exhausted}]\n\noptions:\n  -h, --help            show this help message and exit\n  --binary BINARY\n  --output OUTPUT\n  --fixture-assets-binary FIXTURE_ASSETS_BINARY\n                        native create-soundd-fixture-assets executable\n  --portaudio-library PORTAUDIO_LIBRARY\n                        reuse an already compiled owned PortAudio ABI fixture\n  --mode {live,bounded,retry,inactive,start-failed,stop-retry,exhausted}\n";
fn display(s: &OsStr) -> String {
    crate::python_fs::codepoints(s)
        .into_iter()
        .map(|p| char::from_u32(p).map_or_else(|| format!("\\u{p:04x}"), |c| c.to_string()))
        .collect()
}
fn error(s: impl std::fmt::Display) -> Box<dyn std::error::Error> {
    Box::new(ArgumentError(format!(
        "{USAGE}\ncheck-soundd-daemon: error: {s}"
    )))
}
fn option(s: &OsStr) -> bool {
    let b = s.as_bytes();
    if b == b"-" || !b.starts_with(b"-") || b.contains(&b' ') {
        return false;
    }
    let s = display(s);
    let body = s[1..].strip_suffix('\n').unwrap_or(&s[1..]);
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
/// Python PosixPath spelling: preserve `..` and raw bytes, remove `.` and empty parts.
fn lexical(s: &OsStr) -> PathBuf {
    let b = s.as_bytes();
    let mut out = Vec::new();
    if b.starts_with(b"//") && !b.starts_with(b"///") {
        out.extend_from_slice(b"//");
    } else if b.starts_with(b"/") {
        out.push(b'/');
    }
    for part in b
        .split(|&v| v == b'/')
        .filter(|v| !v.is_empty() && *v != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    PathBuf::from(OsString::from_vec(out))
}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: BTreeMap<OsString, OsString>,
) -> Result<Command> {
    const NAMES: &[&[u8]] = &[
        b"--help",
        b"--binary",
        b"--output",
        b"--fixture-assets-binary",
        b"--portaudio-library",
        b"--mode",
    ];
    let mut values: [Option<OsString>; 5] = Default::default();
    let mut extras = Vec::new();
    let mut literal = false;
    let mut i = 0;
    // Like argparse, classify ambiguity before executing a help action.
    for arg in args.iter().take_while(|a| a.as_bytes() != b"--") {
        let name = arg
            .as_bytes()
            .split(|&b| b == b'=')
            .next()
            .unwrap_or_default();
        if name.starts_with(b"--") && NAMES.iter().filter(|n| n.starts_with(name)).count() > 1 {
            return Err(error(format!("ambiguous option: {}", display(arg))));
        }
    }
    while i < args.len() {
        let arg = &args[i];
        let raw = arg.as_bytes();
        if raw == b"--" {
            literal = true;
            i += 1;
            continue;
        }
        let name = raw.split(|&b| b == b'=').next().unwrap_or_default();
        let short = name.starts_with(b"-h") && !name.starts_with(b"--");
        let which = if literal {
            None
        } else if short {
            Some(0)
        } else {
            NAMES
                .iter()
                .position(|n| *n == name || (name.starts_with(b"--") && n.starts_with(name)))
        };
        if let Some(which) = which {
            let inline = if short && !raw.starts_with(b"-h=") {
                None
            } else {
                raw.iter()
                    .position(|&b| b == b'=')
                    .map(|p| OsString::from_vec(raw[p + 1..].to_vec()))
            };
            if which == 0 {
                if let Some(v) = inline {
                    return Err(error(format!(
                        "argument -h/--help: ignored explicit argument '{}'",
                        display(&v)
                    )));
                }
                return Ok(Command::Help);
            }
            let value = if let Some(v) = inline {
                v
            } else {
                i += 1;
                args.get(i).filter(|v| !option(v)).cloned().ok_or_else(|| {
                    error(format!(
                        "argument {}: expected one argument",
                        String::from_utf8_lossy(NAMES[which])
                    ))
                })?
            };
            if which == 5 && !Mode::ALL.iter().any(|m| value == m.as_str()) {
                return Err(error(format!(
                    "argument --mode: invalid choice: '{}' (choose from {CHOICES})",
                    display(&value)
                )));
            }
            values[which - 1] = Some(value);
        } else {
            extras.push(arg.clone());
        }
        i += 1;
    }
    let missing = (0..3)
        .filter(|&i| values[i].is_none())
        .map(|i| String::from_utf8_lossy(NAMES[i + 1]))
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
                .map(|v| display(v))
                .collect::<Vec<_>>()
                .join(" ")
        )));
    }
    let root = environment
        .get(OsStr::new("OPENPILOT_RUST_ROOT"))
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        );
    let helper = lexical(values[2].as_deref().ok_or("missing assets helper")?);
    let fixture_assets_binary = resolve_non_strict(&if helper.is_absolute() {
        helper
    } else {
        cwd.join(helper)
    })?;
    let modes = if let Some(s) = &values[4] {
        vec![Mode::parse(s.to_str().ok_or("mode is not Unicode")?)?]
    } else {
        Mode::ALL.to_vec()
    };
    require(!modes.is_empty(), "empty soundd roster")?;
    Ok(Command::Run(Options {
        root,
        binary: lexical(values[0].as_deref().ok_or("missing binary")?),
        output: lexical(values[1].as_deref().ok_or("missing output")?),
        fixture_assets_binary,
        portaudio_library: values[3].as_deref().map(lexical),
        modes,
        environment,
    }))
}
