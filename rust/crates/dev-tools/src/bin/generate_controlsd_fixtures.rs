//! JSONL adapter; output files are create-new and no interpreter is launched.
use openpilot_dev_tools::{controlsd_fixture_generate as fixture, seal::Result};
use openpilot_log_format::JsonValue;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};
fn run() -> Result<()> {
    let mut options = BTreeMap::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        let flag = flag.into_string().map_err(|_| "non-UTF-8 option")?;
        if !["--root", "--input", "--output"].contains(&flag.as_str()) {
            return Err("unknown option".into());
        }
        let value = args.next().ok_or("option value missing")?;
        if options.insert(flag, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let mut take = |name: &str| {
        options
            .remove(name)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{name} required"))
    };
    let root = take("--root")?.canonicalize()?;
    let input = take("--input")?;
    let output = take("--output")?;
    fixture::verify_sources(&root)?;
    if !fs::symlink_metadata(&input)?.is_file() {
        return Err("input must be regular".into());
    }
    let input_seal = openpilot_dev_tools::seal::digest(&input)?;
    let held = File::from(rustix::fs::open(
        &input,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?);
    let before = held.metadata()?;
    if !before.is_file() || before.len() > 512 * 1024 * 1024 {
        return Err("input outside regular 512 MiB bound".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let own = file.metadata()?;
    let result = (|| -> Result<()> {
        let mut reader = BufReader::new(held);
        loop {
            let mut line = String::new();
            let count = reader
                .by_ref()
                .take(4 * 1024 * 1024 + 1)
                .read_line(&mut line)?;
            if count == 0 {
                break;
            }
            if count > 4 * 1024 * 1024 {
                return Err("request line exceeds 4 MiB".into());
            }
            let generated = fixture::generate(&JsonValue::parse(&line)?)?;
            writeln!(file, "{}", generated.to_json()?)?;
        }
        file.sync_all()?;
        fixture::verify_sources(&root)?;
        let after = fs::symlink_metadata(&input)?;
        if !after.is_file()
            || (
                before.dev(),
                before.ino(),
                before.len(),
                before.mtime(),
                before.mtime_nsec(),
                before.ctime(),
                before.ctime_nsec(),
            ) != (
                after.dev(),
                after.ino(),
                after.len(),
                after.mtime(),
                after.mtime_nsec(),
                after.ctime(),
                after.ctime_nsec(),
            )
            || openpilot_dev_tools::seal::digest(&input)? != input_seal
        {
            return Err("fixture input changed".into());
        }
        let current = fs::symlink_metadata(&output)?;
        if !current.is_file() || (current.dev(), current.ino()) != (own.dev(), own.ino()) {
            return Err("fixture output replaced".into());
        }
        Ok(())
    })();
    if result.is_err()
        && fs::symlink_metadata(&output).is_ok_and(|m| (m.dev(), m.ino()) == (own.dev(), own.ino()))
    {
        let _ = fs::remove_file(&output);
    }
    result
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("controlsd fixtures: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
