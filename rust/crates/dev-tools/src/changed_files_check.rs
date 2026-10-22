//! Native changed-path selector and explicit Python 3.12 AST admission candidate.
//! Opt-in adapter; maintained Python callers remain until differentials pass.
mod codec_names;
mod decode;
mod idna;
mod japanese;
mod japanese_variants;
mod names15;
mod process;
mod punycode;
pub use process::RunControl;
mod single_byte_tables;
mod unicode15;
mod utf7;
use ruff_python_ast::{token::TokenKind, Mod, PythonVersion};
use ruff_python_parser::{parse_unchecked, Mode, ParseOptions, Parsed};
use std::{
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{self, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::Duration,
};

/// Unsupported native decoding is distinct from an observed Python rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Whitespace,
    Git,
    Cancelled,
    Timeout,
    Read,
    Decode,
    Capability,
    Syntax,
}
#[derive(Debug)]
pub struct Diagnostic {
    pub phase: Phase,
    pub message: String,
    pub path: Option<PathBuf>,
}
impl Diagnostic {
    fn new(phase: Phase, message: impl Into<String>) -> Self {
        Self {
            phase,
            message: message.into(),
            path: None,
        }
    }
    fn at(mut self, path: &Path) -> Self {
        self.path = Some(path.to_owned());
        self
    }
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}: ", path.display())?;
        }
        write!(f, "Python3.12 {:?}: {}", self.phase, self.message)
    }
}
impl std::error::Error for Diagnostic {}

/// Decode a codec payload to exact UTF8 without source newline/NUL processing.
/// This API exposes the native byte boundary for original-provider differentials.
pub fn decode_bytes(raw: &[u8], encoding: &str) -> Result<String, Diagnostic> {
    decode::payload(raw, encoding)
}
pub fn named_character(name: &str) -> Result<char, Diagnostic> {
    names15::lookup(name).ok_or_else(|| {
        Diagnostic::new(
            Phase::Syntax,
            format!("unknown Unicode15 character name: {name}"),
        )
    })
}
fn named_escapes(text: &str) -> Result<(), Diagnostic> {
    // Interpret only unpaired backslashes. Literal \\N is not a named escape.
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'N') && bytes.get(i + 2) == Some(&b'{') {
            let start = i + 3;
            let end = bytes[start..]
                .iter()
                .position(|&b| b == b'}')
                .map(|at| start + at)
                .ok_or_else(|| Diagnostic::new(Phase::Syntax, "unterminated named escape"))?;
            named_character(&text[start..end])?;
            i = end + 1;
        } else {
            i += 2;
        }
    }
    Ok(())
}
/// Parse source bytes without executing Python or applying compiler semantics.
pub fn check_bytes(raw: &[u8]) -> Result<(), Diagnostic> {
    let source = decode::source(raw)?;
    check_text(&source).map(|_| ())
}
fn check_text(source: &str) -> Result<Parsed<Mod>, Diagnostic> {
    let parsed = parse_unchecked(
        source,
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::PY312),
    );
    // into_result/is_ok and has_valid_syntax ignore unsupported-version errors.
    if !parsed.has_no_syntax_errors() {
        return Err(Diagnostic::new(
            Phase::Syntax,
            format!(
                "grammar={:?}; unsupported3.12={:?}",
                parsed.errors(),
                parsed.unsupported_syntax_errors()
            ),
        ));
    }
    for token in parsed.tokens() {
        let (kind, range) = token.as_tuple();
        let text = &source[range.start().to_usize()..range.end().to_usize()];
        if kind == TokenKind::Name {
            for (index, c) in text.chars().enumerate() {
                let ranges = if index == 0 {
                    unicode15::START
                } else {
                    unicode15::CONTINUE
                };
                if !unicode15::contains(ranges, c) {
                    return Err(Diagnostic::new(
                        Phase::Syntax,
                        format!(
                            "character U+{:04X} is not a Python3.12 identifier",
                            u32::from(c)
                        ),
                    ));
                }
            }
        }
        if matches!(kind, TokenKind::String | TokenKind::FStringMiddle) {
            let flags = token.string_flags().expect("selected string token flags");
            if !flags.is_raw_string() && !flags.is_byte_string() {
                named_escapes(text)?;
            }
        }
    }
    Ok(parsed)
}
/// A complete lexical probe, for differential validation against CPython's full
/// independent Unicode15 start/continue property corpus (not a global char ban).
#[doc(hidden)]
pub fn identifier_is_python312(identifier: &str) -> bool {
    let source = format!("{identifier}=1\n");
    check_text(&source).is_ok_and(|parsed| {
        parsed.tokens().iter().any(|token| {
            let (kind, range) = token.as_tuple();
            kind == TokenKind::Name
                && range.start().to_usize() == 0
                && range.end().to_usize() == identifier.len()
        })
    })
}
#[derive(Debug)]
pub struct Report {
    pub base: OsString,
    pub checked_paths: Vec<PathBuf>,
}
fn git(root: &Path, args: &[&OsStr], control: &RunControl) -> Result<Vec<u8>, Diagnostic> {
    process::git(root, args, false, Phase::Git, control)
}
/// Same NUL-separated ACMR selection and current worktree bytes as the original.
pub fn run(root: &Path, explicit_base: Option<OsString>) -> Result<Report, Diagnostic> {
    let control = RunControl::new(Duration::from_secs(120))?;
    run_controlled(root, explicit_base, &control)
}
/// Caller-controlled cancellation/deadline; no global signal handlers are
/// installed by this library entrypoint. Parsing checks control between files.
pub fn run_controlled(
    root: &Path,
    explicit_base: Option<OsString>,
    control: &RunControl,
) -> Result<Report, Diagnostic> {
    control.check()?;
    let mut base = explicit_base
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os("BASE_SHA"))
        .unwrap_or_default();
    if base.is_empty() || base.as_bytes().iter().all(|&v| v == b'0') {
        let output = git(
            root,
            &[
                OsStr::new("merge-base"),
                OsStr::new("origin/dev"),
                OsStr::new("HEAD"),
            ],
            control,
        )?;
        let bytes = output.strip_suffix(b"\n").unwrap_or(&output);
        base = OsString::from_vec(bytes.to_vec());
    }
    // Preserve failure ordering: whitespace rejection must precede AST admission.
    process::git(
        root,
        &[
            OsStr::new("diff"),
            OsStr::new("--check"),
            &base,
            OsStr::new("HEAD"),
        ],
        true,
        Phase::Whitespace,
        control,
    )?;
    let selected = git(
        root,
        &[
            OsStr::new("diff"),
            OsStr::new("--name-only"),
            OsStr::new("-z"),
            OsStr::new("--diff-filter=ACMR"),
            &base,
            OsStr::new("HEAD"),
        ],
        control,
    )?;
    let mut checked_paths = Vec::new();
    for raw in selected.split(|&v| v == 0).filter(|v| !v.is_empty()) {
        control.check()?;
        let path = PathBuf::from(OsString::from_vec(raw.to_vec()));
        if path.extension() != Some(OsStr::new("py")) {
            continue;
        }
        let absolute = root.join(&path);
        if !fs::symlink_metadata(&absolute)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        {
            continue;
        }
        let raw = fs::read(&absolute)
            .map_err(|e| Diagnostic::new(Phase::Read, e.to_string()).at(&path))?;
        check_bytes(&raw).map_err(|error| error.at(&path))?;
        checked_paths.push(path);
    }
    control.check()?;
    Ok(Report {
        base,
        checked_paths,
    })
}
pub fn cli() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut base = None;
    while let Some(arg) = args.next() {
        if arg == "--base" && base.is_none() {
            base = Some(args.next().ok_or("--base requires a value")?);
        } else if arg == "--help" || arg == "-h" {
            println!("Usage: check-changed-files [--base SHA] (BASE_SHA or origin/dev fallback)");
            return Ok(());
        } else {
            return Err(format!("unknown or duplicate option: {}", arg.to_string_lossy()).into());
        }
    }
    let control = RunControl::new(Duration::from_secs(120))?;
    let _signals = process::Signals::install(&control)?;
    let report = run_controlled(&std::env::current_dir()?, base, &control)?;
    let mut out = io::stdout().lock();
    write!(
        out,
        "Whitespace checked; parsed {} changed Python files against ",
        report.checked_paths.len()
    )?;
    out.write_all(report.base.as_bytes())?;
    writeln!(out)?;
    Ok(())
}
