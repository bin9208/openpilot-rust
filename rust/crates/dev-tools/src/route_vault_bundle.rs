//! Port of tools/carrot_route_vault/build_bundle.py (repository licensing applies).
//! Committed context packaging only; workflow activation and Python deletion HELD.
use crate::{
    argparse_scan::{self, OptionSpec, ParsedOs},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use std::{
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{self, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub const SOURCES: [&str; 13] = [
    "openpilot/common/filter_simple.py",
    "openpilot/selfdrive/carrot/radar/lateral.py",
    "openpilot/selfdrive/carrot/radar/tools/radar_group3_replay.py",
    "openpilot/selfdrive/carrot/radar_motion",
    "openpilot/selfdrive/carrot/radar/tools/radar_validation_replay.py",
    "openpilot/selfdrive/carrot/radar/tools/radar_web_export.py",
    "openpilot/selfdrive/carrot/cluster",
    "openpilot/selfdrive/controls/lib/cutin_alert.py",
    "openpilot/selfdrive/controls/lib/cutin_helpers.py",
    "openpilot/cereal",
    "opendbc_repo/opendbc/__init__.py",
    "opendbc_repo/opendbc/car",
    "opendbc_repo/opendbc/dbc",
];
const FIXED: [&str; 7] = [
    "server.py",
    "viewer.py",
    "radar.py",
    "radar_view.js",
    "requirements.txt",
    "Dockerfile.vault",
    "deploy_probe.py",
];
#[derive(Debug)]
pub struct GitFailure {
    pub argv: Vec<OsString>,
    pub code: i32,
}
impl fmt::Display for GitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CalledProcessError: {:?} returned status {}",
            self.argv, self.code
        )
    }
}
impl std::error::Error for GitFailure {}

/// pathlib's lexical Unix spelling: collapse empty/'.' components but retain
/// '..', symlinks, and exactly two leading slashes. No filesystem resolution.
fn path(value: &Path) -> PathBuf {
    let bytes = value.as_os_str().as_bytes();
    let mut out = Vec::new();
    if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        out.extend_from_slice(b"//");
    } else if bytes.starts_with(b"/") {
        out.push(b'/');
    }
    for part in bytes
        .split(|&b| b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
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
/// Match Path.mkdir(parents=True, exist_ok=...) target-first/retry behavior.
/// Do not use create_dir_all for the exclusive destination itself.
fn mkdir(value: &Path, exist_ok: bool) -> io::Result<()> {
    match fs::create_dir(value) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = value
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            if parent == value {
                return Err(error);
            }
            mkdir(parent, true)?;
            fs::create_dir(value).or_else(|error| {
                if exist_ok && value.is_dir() {
                    Ok(())
                } else {
                    Err(error)
                }
            })
        }
        Err(_) if exist_ok && value.is_dir() => Ok(()),
        Err(error) => Err(error),
    }
}
fn selected(name: &str) -> bool {
    let name = path(Path::new(name));
    let Some(name) = name.file_name() else {
        return false;
    };
    let name = name.as_bytes();
    let Some(dot) = name.iter().rposition(|&b| b == b'.') else {
        return false;
    };
    dot > 0 && matches!(&name[dot..], b".py" | b".capnp" | b".dbc")
}
fn splitlines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r' | '\x0b' | '\x0c' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            out.push(&text[start..at]);
            start = at + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                start = chars.next().unwrap().0 + 1;
            }
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}
fn git(root: &Path, args: Vec<OsString>, runner: &mut dyn Runner, text: bool) -> Result<Vec<u8>> {
    let mut argv = vec![OsString::from("git")];
    argv.extend(args);
    // Original check_output has no application timeout and inherits environment,
    // stdin and stderr. Keep Git quoting/config policy; never add -z or unquote.
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::MAX);
    spec.cwd = Some(root.into());
    spec.inherit_stdin = true;
    spec.inherit_stderr = true;
    spec.stdout_limit = usize::MAX;
    let output = runner.run(&spec)?;
    // Popen text decoding happens during communicate(), before check_returncode.
    if text {
        std::str::from_utf8(&output.stdout)?;
    }
    if output.code != 0 {
        return Err(GitFailure {
            argv,
            code: output.code,
        }
        .into());
    }
    Ok(output.stdout)
}
/// ROOT is explicit for tests. Destination is relative to the caller's cwd,
/// just like the original Path argument, while every Git child runs in ROOT.
/// Failed commands retain prior directories/files; there is deliberately no rollback.
pub fn build(
    root: &Path,
    destination: &Path,
    reference: &OsStr,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
) -> Result<()> {
    let destination = path(destination);
    mkdir(&destination, false)?;
    let mut reference = reference.to_os_string();
    reference.push("^{commit}");
    let commit = git(
        root,
        vec!["rev-parse".into(), "--verify".into(), reference],
        runner,
        true,
    )?;
    let commit = std::str::from_utf8(&commit)?
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    let mut args = vec![
        "ls-tree".into(),
        "-r".into(),
        "--name-only".into(),
        commit.into(),
        "--".into(),
    ];
    args.extend(SOURCES.map(OsString::from));
    let listing = git(root, args, runner, true)?;
    let listing = std::str::from_utf8(&listing)?;
    for name in splitlines(listing) {
        if !selected(name) {
            continue;
        }
        let target = path(&destination.join(name));
        mkdir(target.parent().unwrap_or(Path::new(".")), true)?;
        // Evaluate check_output before write_bytes opens/truncates the target.
        let blob = git(
            root,
            vec!["show".into(), format!("{commit}:{name}").into()],
            runner,
            false,
        )?;
        fs::write(target, blob)?;
    }
    for name in FIXED {
        let blob = git(
            root,
            vec![
                "show".into(),
                format!("{commit}:tools/carrot_route_vault/{name}").into(),
            ],
            runner,
            false,
        )?;
        fs::write(destination.join(name), blob)?;
    }
    fs::write(destination.join("SOURCE_COMMIT"), format!("{commit}\n"))?;
    let mut line = b"Built ".to_vec();
    line.extend_from_slice(destination.as_os_str().as_bytes());
    line.extend_from_slice(format!(" from {commit}\n").as_bytes());
    stdout.write_all(&line)?;
    Ok(())
}
pub fn parse(args: &[OsString]) -> std::result::Result<ParsedOs, argparse_scan::ArgumentError> {
    argparse_scan::scan_os_with_positionals(args, &[OptionSpec::value("--ref", false)], 1)
}
/// Source-root convention, not cwd/executable discovery. A moved binary still
/// reads the checkout it was compiled from; use library build for explicit roots.
pub fn cli_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
const HELP:&str="usage: build-route-vault-bundle [-h] [--ref REF] destination\n\nBuild a DSM Docker context entirely from committed replay and viewer sources.\n\nUncommitted vehicle/controller edits are deliberately excluded.\n\npositional arguments:\n  destination\n\noptions:\n  -h, --help  show this help message and exit\n  --ref REF\n";
pub fn cli() -> i32 {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = match parse(&args) {
        Ok(p) => p,
        Err(error) => {
            eprintln!("{}\nerror: {error}", HELP.lines().next().unwrap());
            return 2;
        }
    };
    let result = if parsed.help {
        io::stdout()
            .lock()
            .write_all(HELP.as_bytes())
            .map_err(Into::into)
    } else {
        let reference = parsed
            .values
            .get("--ref")
            .map(OsString::as_os_str)
            .unwrap_or(OsStr::new("HEAD"));
        build(
            &cli_root(),
            Path::new(&parsed.positionals[0]),
            reference,
            &mut NativeRunner,
            &mut io::stdout().lock(),
        )
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("build-route-vault-bundle: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pathlib_spelling_never_resolves_parent_components_or_links() {
        for (raw, expected) in [
            ("", "."),
            ("./a//b/../", "a/b/.."),
            ("//a///b", "//a/b"),
            ("///a//b", "/a/b"),
            (".py", ".py"),
        ] {
            assert_eq!(
                path(Path::new(raw)).as_os_str().as_bytes(),
                expected.as_bytes()
            );
        }
        for name in ["a.py", "a.capnp", "a.dbc", "..py", "a.py/."] {
            assert!(selected(name));
        }
        for name in [".py", "a.py.", "A.PY", "\"a.py\"", "a.py/..", ""] {
            assert!(!selected(name));
        }
    }
}
