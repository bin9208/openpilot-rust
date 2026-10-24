//! Two-phase, source-authored executable-provenance audit for the frozen
//! eight-controller recovery family. The parent workflow owns registration.

use openpilot_dev_tools::recovery_family_exec_policy::{
    audit_after_harness, capture_preflight, SelectedElfs,
};
use std::{collections::BTreeMap, env, ffi::OsString, path::PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn options() -> Result<(OsString, BTreeMap<OsString, OsString>)> {
    let mut args = env::args_os();
    let _program = args.next().ok_or("missing audit executable")?;
    let command = args.next().ok_or("expected pre or post")?;
    let mut values = BTreeMap::new();
    while let Some(option) = args.next() {
        let name = option.to_str().ok_or("non-UTF8 audit option")?;
        if !name.starts_with("--") {
            return Err("unexpected recovery audit argument".into());
        }
        let value = args.next().ok_or("missing recovery audit option value")?;
        if values.insert(option, value).is_some() {
            return Err("duplicate recovery audit option".into());
        }
    }
    Ok((command, values))
}

fn take(values: &mut BTreeMap<OsString, OsString>, name: &str) -> Result<OsString> {
    values
        .remove(&OsString::from(name))
        .ok_or_else(|| format!("missing {name}").into())
}

fn path(values: &mut BTreeMap<OsString, OsString>, name: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(take(values, name)?))
}

fn string(values: &mut BTreeMap<OsString, OsString>, name: &str) -> Result<String> {
    Ok(take(values, name)?
        .into_string()
        .map_err(|_| format!("{name} is not UTF-8"))?)
}

fn run() -> Result<()> {
    let (command, mut values) = options()?;
    match command.to_str() {
        Some("pre") => {
            let output = path(&mut values, "--receipt")?;
            let native_root = path(&mut values, "--native-root")?;
            let selected = SelectedElfs {
                server: path(&mut values, "--server")?,
                actions: path(&mut values, "--actions")?,
                upload: path(&mut values, "--upload")?,
                child: path(&mut values, "--child")?,
                cwp: path(&mut values, "--cwp")?,
                tmux: path(&mut values, "--tmux")?,
                harness: path(&mut values, "--harness")?,
            };
            let host_path = string(&mut values, "--host-path")?;
            if !values.is_empty() {
                return Err("unknown recovery preflight option".into());
            }
            let digest = capture_preflight(&output, &native_root, selected, host_path)?;
            println!("{digest}");
        }
        Some("post") => {
            let preflight = path(&mut values, "--receipt")?;
            let preflight_sha256 = string(&mut values, "--receipt-sha256")?;
            let trace_dir = path(&mut values, "--trace-dir")?;
            let output = path(&mut values, "--output")?;
            let harness_exit_code = string(&mut values, "--harness-exit")?.parse()?;
            if !values.is_empty() {
                return Err("unknown recovery postflight option".into());
            }
            audit_after_harness(
                &preflight,
                &preflight_sha256,
                &trace_dir,
                &output,
                harness_exit_code,
            )?;
        }
        _ => return Err("expected recovery audit pre or post".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("recovery exec audit failed: {error}");
        std::process::exit(1);
    }
}
