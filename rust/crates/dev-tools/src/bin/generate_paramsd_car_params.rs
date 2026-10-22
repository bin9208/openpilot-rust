//! Raw CarParams entrypoint for the paramsd atomic startup gate. The ported
//! builder already passed independent seven-scenario/loop/daemon wire gates.
use openpilot_dev_tools::{paramsd_fixture_generate::car_params, seal::Result};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::PathBuf,
};
const FIXTURE: &str = "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521";

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut options = BTreeMap::new();
    while let Some(flag) = args.next() {
        let flag = flag.into_string().map_err(|_| "non-UTF-8 option")?;
        if !matches!(
            flag.as_str(),
            "--root" | "--output" | "--profile" | "--fingerprint"
        ) {
            return Err("unknown option".into());
        }
        let value = args.next().ok_or("option value missing")?;
        if options.insert(flag, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let root = PathBuf::from(options.remove("--root").ok_or("--root required")?).canonicalize()?;
    let output = PathBuf::from(options.remove("--output").ok_or("--output required")?);
    let profile = options
        .remove("--profile")
        .map(|raw| raw.into_string().map_err(|_| "non-UTF-8 profile"))
        .transpose()?
        .unwrap_or_else(|| "0".into())
        .parse::<usize>()?;
    if profile > 2 {
        return Err("profile must be 0, 1 or 2".into());
    }
    let fingerprint = options
        .remove("--fingerprint")
        .map(|raw| raw.into_string().map_err(|_| "non-UTF-8 fingerprint"))
        .transpose()?;
    let source = root.join("rust/tools/paramsd_fixture.py");
    if !fs::symlink_metadata(&source)?.is_file() {
        return Err("nonregular original fixture".into());
    }
    let held = File::from(rustix::fs::open(
        &source,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )?);
    let before = held.metadata()?;
    if format!("{:x}", Sha256::digest(fs::read(&source)?)) != FIXTURE {
        return Err("original paramsd fixture changed".into());
    }
    let bytes = car_params(profile, fingerprint.as_deref());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let metadata = file.metadata()?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        let after = fs::symlink_metadata(&source)?;
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
            || format!("{:x}", Sha256::digest(fs::read(&source)?)) != FIXTURE
        {
            return Err("original paramsd fixture changed during generation".into());
        }
        let current = fs::symlink_metadata(&output)?;
        if !current.is_file()
            || (current.dev(), current.ino()) != (metadata.dev(), metadata.ino())
            || fs::read(&output)? != bytes
        {
            return Err("generated CarParams identity changed".into());
        }
        Ok(())
    })();
    if result.is_err()
        && fs::symlink_metadata(&output)
            .is_ok_and(|m| (m.dev(), m.ino()) == (metadata.dev(), metadata.ino()))
    {
        let _ = fs::remove_file(&output);
    }
    result
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("generate paramsd CarParams: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
