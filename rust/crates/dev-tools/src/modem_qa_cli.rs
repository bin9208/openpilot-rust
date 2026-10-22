//! Native replacement entry point for the modem source/PTY checker.
use crate::{
    argparse_scan::{self, ArgumentError, OptionSpec},
    modem_qa::{self, TraceOptions},
    seal::Result,
};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
};

fn args(values: &[OsString]) -> std::result::Result<argparse_scan::ParsedOs, ArgumentError> {
    argparse_scan::scan_os(
        values,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--trace", true),
            OptionSpec::value("--evidence", true),
            OptionSpec::value("--source-root", false),
            OptionSpec::value("--launcher", false),
            OptionSpec::flag("--require-retired", None),
        ],
    )
}

fn emit(value: &Value) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

/// Run the native original-source differential and continuous daemon fixture.
pub fn cli() -> Result<()> {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = args(&raw)?;
    if parsed.help {
        writeln!(
            io::stdout().lock(),
            "check-modem --binary ELF --trace ELF --evidence PATH [--launcher ELF] [--source-root PATH] [--require-retired]"
        )?;
        return Ok(());
    }
    let daemon = PathBuf::from(&parsed.values["--binary"]).canonicalize()?;
    let trace = PathBuf::from(&parsed.values["--trace"]).canonicalize()?;
    let launcher = parsed
        .values
        .get("--launcher")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            daemon
                .parent()
                .unwrap_or(Path::new("."))
                .join("openpilot-process-child")
        })
        .canonicalize()?;
    let source_root = parsed
        .values
        .get("--source-root")
        .map_or_else(std::env::current_dir, |path| Ok(PathBuf::from(path)))?
        .canonicalize()?;
    let helper = std::env::current_exe()?.canonicalize()?;
    let options = TraceOptions {
        source_root,
        daemon,
        helper,
        trace,
        launcher,
        output: PathBuf::from(&parsed.values["--evidence"]),
        require_retired: parsed.values.contains_key("--require-retired"),
    };
    modem_qa::run_with_emit(&options, emit).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checker_cli_requires_all_runtime_inputs_and_accepts_help() {
        assert!(args(&["--help".into()]).unwrap().help);
        assert!(args(&[]).is_err());
        assert!(args(&["--binary".into(), "x".into()]).is_err());
    }
}
