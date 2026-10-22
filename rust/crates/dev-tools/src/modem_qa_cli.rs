//! Native replacement entry point for the modem source/PTY checker.
use crate::{
    argparse_scan::{self, ArgumentError, OptionSpec},
    modem_capture::SourceState,
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
            OptionSpec::flag("--require-tools-retired", None),
        ],
    )
}

fn source_state(
    parsed: &argparse_scan::ParsedOs,
) -> std::result::Result<SourceState, ArgumentError> {
    let all_retired = parsed.values.contains_key("--require-retired");
    let tools_retired = parsed.values.contains_key("--require-tools-retired");
    if all_retired && tools_retired {
        return Err(ArgumentError(
            "--require-retired and --require-tools-retired are mutually exclusive".into(),
        ));
    }
    Ok(if all_retired {
        SourceState::AllRetired
    } else if tools_retired {
        SourceState::ToolsRetiredRuntimePinned
    } else {
        SourceState::OriginalPresent
    })
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
            "check-modem --binary ELF --trace ELF --evidence PATH [--launcher ELF] [--source-root PATH] [--require-tools-retired | --require-retired]"
        )?;
        return Ok(());
    }
    let source_state = source_state(&parsed)?;
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
        source_state,
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

    #[test]
    fn checker_cli_exposes_three_exclusive_source_states() {
        let base = [
            "--binary".into(),
            "daemon".into(),
            "--trace".into(),
            "trace".into(),
            "--evidence".into(),
            "out".into(),
        ];
        assert_eq!(
            source_state(&args(&base).unwrap()).unwrap(),
            SourceState::OriginalPresent
        );
        let mut tools = base.to_vec();
        tools.push("--require-tools-retired".into());
        assert_eq!(
            source_state(&args(&tools).unwrap()).unwrap(),
            SourceState::ToolsRetiredRuntimePinned
        );
        let mut all = base.to_vec();
        all.push("--require-retired".into());
        assert_eq!(
            source_state(&args(&all).unwrap()).unwrap(),
            SourceState::AllRetired
        );
        let mut conflicting = tools;
        conflicting.push("--require-retired".into());
        assert!(source_state(&args(&conflicting).unwrap()).is_err());
    }
}
