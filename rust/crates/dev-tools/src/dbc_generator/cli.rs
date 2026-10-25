use super::{Error, Program, Result};
use std::{
    ffi::{OsStr, OsString},
    io::{self, Write},
    path::{Path, PathBuf},
};
const HELP:&str="usage: dbc-generator compose SOURCE FILENAME OUTPUT\n       dbc-generator create-all OUTPUT --source SOURCE [--all-programs | --program NAME ...]\n       dbc-generator render PROGRAM DIRECTORY\n       dbc-generator generate-all [--source SOURCE]\n       dbc-generator program-outputs\n";
fn default_source() -> Result<PathBuf> {
    let root = if let Some(root) = std::env::var_os("OPENPILOT_RUST_ROOT") {
        PathBuf::from(root)
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    };
    Ok(root.join("opendbc_repo/opendbc/dbc/generator"))
}
fn stdout(text: &str) -> Result<()> {
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| Error::new("OSError", e.raw_os_error(), &e.to_string()))
}
fn options(args: &[OsString], programs_allowed: bool) -> Result<(PathBuf, Vec<Program>, bool)> {
    let mut source = None;
    let mut programs = Vec::new();
    let mut all_programs = false;
    let mut i = 0;
    while i < args.len() {
        let option = &args[i];
        if option == "--all-programs" && programs_allowed {
            all_programs = true;
            i += 1;
            continue;
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| Error::argument("option needs a value"))?;
        if option == "--source" {
            source = Some(PathBuf::from(value));
        } else if option == "--program" && programs_allowed {
            programs.push(Program::parse(
                value
                    .to_str()
                    .ok_or_else(|| Error::argument("native program names are UTF8"))?,
            )?);
        } else {
            return Err(Error::argument(&format!("unrecognized option {option:?}")));
        }
        i += 2;
    }
    if all_programs && !programs.is_empty() {
        return Err(Error::argument(
            "--all-programs cannot be mixed with --program",
        ));
    }
    Ok((
        match source {
            Some(s) => s,
            None => default_source()?,
        },
        programs,
        all_programs,
    ))
}
pub fn generator_cli() -> Result<()> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--json-errors") {
        args.remove(0);
    }
    if args.first().is_some_and(|a| a == "--help" || a == "-h") {
        return stdout(HELP);
    }
    let operation = args
        .first()
        .and_then(|v| v.to_str())
        .ok_or_else(|| Error::argument(HELP))?;
    match operation {
        "compose" if args.len() == 4 => {
            super::create_dbc(
                Path::new(&args[1]),
                Path::new(&args[2]),
                Path::new(&args[3]),
            )?;
            Ok(())
        }
        "render" if args.len() == 3 => super::render_program(
            Program::parse(
                args[1]
                    .to_str()
                    .ok_or_else(|| Error::argument("native program names are UTF8"))?,
            )?,
            Path::new(&args[2]),
        ),
        "create-all" if args.len() >= 2 => {
            let (source, programs, all) = options(&args[2..], true)?;
            if all {
                super::create_all_complete(Path::new(&args[1]), &source)
            } else {
                super::create_all(Path::new(&args[1]), &source, &programs)
            }
        }
        "generate-all" => {
            let (source, _, _) = options(&args[1..], false)?;
            stdout(&(super::generated_json(&source)? + "\n"))
        }
        "program-outputs" if args.len() == 1 => stdout(
            &(serde_json::to_string(&super::program_outputs()).expect("registry strings") + "\n"),
        ),
        _ => Err(Error::argument(HELP)),
    }
}
pub fn stage_cli() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--help" || a == "-h") {
        return stdout("usage: stage-control-dbcs [--root DBC_ROOT] [--source GENERATOR_ROOT]\n");
    }
    let mut root = None;
    let mut source = None;
    let mut i = 0;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| Error::argument("option needs a path"))?;
        if args[i] == OsStr::new("--root") {
            root = Some(PathBuf::from(value));
        } else if args[i] == OsStr::new("--source") {
            source = Some(PathBuf::from(value));
        } else {
            return Err(Error::argument("unrecognized stage option"));
        }
        i += 2;
    }
    let source = match source {
        Some(s) => s,
        None => default_source()?,
    };
    let root = match root {
        Some(r) => r,
        None => source
            .parent()
            .ok_or_else(|| Error::argument("source has no parent"))?
            .to_owned(),
    };
    stdout(&super::stage(&root, &source)?.json_line())
}
