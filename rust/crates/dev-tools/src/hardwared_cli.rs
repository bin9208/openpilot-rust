//! Native entry points retaining the original hardwared checker argument forms.
use crate::{
    argparse_scan::{self, ArgumentError, OptionSpec},
    hardwared_qa::{self, DaemonOptions, TraceOptions},
    seal::Result,
};
use serde_json::Value;
use std::{
    ffi::{OsStr, OsString},
    io::{self, Write},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
};

fn source_root(parsed: &argparse_scan::ParsedOs) -> Result<PathBuf> {
    Ok(parsed
        .values
        .get("--source-root")
        .map_or_else(std::env::current_dir, |path| Ok(PathBuf::from(path)))?
        .canonicalize()?)
}

fn resolve_runner(arguments: &[OsString]) -> Result<Vec<OsString>> {
    let Some(program) = arguments.first() else {
        return Ok(Vec::new());
    };
    let executable = |path: &Path| {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    let path = if program.as_bytes().contains(&b'/') {
        PathBuf::from(program)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(program))
            .find(|path| executable(path))
            .ok_or("hardwared runner is not on PATH")?
    };
    if !executable(&path) {
        return Err("hardwared runner is not executable".into());
    }
    let mut resolved = arguments.to_vec();
    resolved[0] = path.canonicalize()?.into_os_string();
    Ok(resolved)
}

fn emit(value: &Value) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn trace_args(args: &[OsString]) -> std::result::Result<argparse_scan::ParsedOs, ArgumentError> {
    argparse_scan::scan_os_with_positionals(
        args,
        &[
            OptionSpec::value("--source-root", false),
            OptionSpec::flag("--require-retired", None),
            OptionSpec::append("--runner"),
        ],
        2,
    )
}
fn daemon_args(args: &[OsString]) -> std::result::Result<argparse_scan::ParsedOs, ArgumentError> {
    argparse_scan::scan_os(
        args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--source-root", false),
            OptionSpec::value("--launcher", false),
            OptionSpec::flag("--require-retired", None),
        ],
    )
}

pub fn trace_cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = trace_args(&args)?;
    if parsed.help {
        writeln!(
            io::stdout().lock(),
            "check-hardwared BINARY OUTPUT [--runner ARG] [--source-root PATH] [--require-retired]"
        )?;
        return Ok(());
    }
    let options = TraceOptions {
        source_root: source_root(&parsed)?,
        binary: PathBuf::from(&parsed.positionals[0]).canonicalize()?,
        output: PathBuf::from(&parsed.positionals[1]),
        runner: resolve_runner(
            parsed
                .append_values
                .get("--runner")
                .map_or(&[], Vec::as_slice),
        )?,
        require_retired: parsed.values.contains_key("--require-retired"),
    };
    hardwared_qa::run_trace_with_emit(&options, emit)?;
    Ok(())
}

pub fn daemon_cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = daemon_args(&args)?;
    if parsed.help {
        writeln!(io::stdout().lock(), "check-hardwared-daemon --binary ELF --output PATH [--launcher ELF] [--source-root PATH] [--require-retired]")?;
        return Ok(());
    }
    let binary = PathBuf::from(&parsed.values["--binary"]).canonicalize()?;
    let launcher = parsed
        .values
        .get("--launcher")
        .map_or_else(
            || {
                binary
                    .parent()
                    .unwrap_or(Path::new(OsStr::new(".")))
                    .join("openpilot-process-child")
            },
            PathBuf::from,
        )
        .canonicalize()?;
    let temporary = tempfile::Builder::new()
        .prefix("hardwared-owned-")
        .tempdir()?;
    let options = DaemonOptions {
        source_root: source_root(&parsed)?,
        binary,
        root: temporary.path().join("root"),
        output: PathBuf::from(&parsed.values["--output"]),
        launcher,
        require_retired: parsed.values.contains_key("--require-retired"),
    };
    hardwared_qa::run_daemon_with_emit(&options, emit)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    #[test]
    fn original_trace_grammar_keeps_repeated_runner_arguments_as_raw_bytes() {
        let runner = OsString::from_vec(b"/target-\xff".to_vec());
        let args = vec![
            "binary".into(),
            "output".into(),
            "--runner".into(),
            "/usr/bin/env".into(),
            "--runner=-u".into(),
            "--runner".into(),
            runner.clone(),
        ];
        let parsed = trace_args(&args).unwrap();
        assert_eq!(
            parsed.positionals,
            vec![OsString::from("binary"), OsString::from("output")]
        );
        assert_eq!(
            parsed.append_values["--runner"],
            vec![OsString::from("/usr/bin/env"), OsString::from("-u"), runner]
        );
    }
    #[test]
    fn help_and_argument_errors_preserve_admission_categories() {
        assert!(trace_args(&["--help".into()]).unwrap().help);
        assert!(daemon_args(&["--help".into()]).unwrap().help);
        assert!(trace_args(&[]).is_err());
        assert!(daemon_args(&[]).is_err());
        assert!(trace_args(&["b".into(), "o".into(), "--unknown".into()]).is_err());
        assert!(daemon_args(&[
            "--binary".into(),
            "b".into(),
            "--output".into(),
            "o".into(),
            "--unknown".into()
        ])
        .is_err());
    }
}
