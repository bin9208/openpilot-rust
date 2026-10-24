use openpilot_dev_tools::{release_files, seal::Result};
use std::{
    io::{self, Write},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|v| v.to_str()) {
        Some("--help" | "-h") => {
            println!("Usage: release-files --root ROOT");
            return Ok(());
        }
        Some("--root") => (),
        _ => return Err("--root ROOT required".into()),
    }
    let root = PathBuf::from(args.next().ok_or("--root requires a value")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let mut output = io::stdout().lock();
    for path in release_files::list(&root)? {
        output.write_all(path.as_os_str().as_bytes())?;
        output.write_all(b"\n")?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Release files: {error}");
        std::process::exit(1);
    }
}
