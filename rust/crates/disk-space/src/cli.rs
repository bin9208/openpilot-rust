use crate::{self as disk_space, Result};
use std::{io::Write, path::PathBuf};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut path = None;
    let mut growth = None;
    while let Some(arg) = args.next() {
        match arg.to_str().ok_or("invalid argument")? {
            "--path" if path.is_none() => {
                path = Some(PathBuf::from(args.next().ok_or("--path needs a value")?));
            }
            "--growth-mib" if growth.is_none() => {
                growth = Some(
                    args.next()
                        .ok_or("--growth-mib needs a value")?
                        .to_str()
                        .ok_or("invalid growth")?
                        .parse::<u64>()?
                        .checked_mul(1024 * 1024)
                        .ok_or("growth exceeds u64")?,
                );
            }
            "--help" | "-h" => {
                println!("Usage: check-disk-space --path PATH --growth-mib NONNEGATIVE_INTEGER");
                return Ok(());
            }
            _ => return Err("unknown or duplicate argument".into()),
        }
    }
    let space = disk_space::inspect(
        &path.ok_or("--path required")?,
        growth.ok_or("--growth-mib required")?,
    )?;
    println!("{}", serde_json::to_string(&space)?);
    std::io::stdout().flush()?;
    space.require()
}

pub fn main() {
    if let Err(error) = run() {
        eprintln!("disk space: {error}");
        std::process::exit(1);
    }
}
