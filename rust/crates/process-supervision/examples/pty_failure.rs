use openpilot_process_supervision::{pty::Pair, CapturedCommand};
use std::{
    fs::File,
    io::{self, BufRead},
    path::PathBuf,
};

fn descriptors() -> io::Result<usize> {
    Ok(std::fs::read_dir("/proc/self/fd")?.count())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let line = io::stdin()
        .lock()
        .lines()
        .next()
        .ok_or("fixture input missing")??;
    let input: serde_json::Value = serde_json::from_str(&line)?;
    let baseline = descriptors()?;
    if input["mode"] == "allocation" {
        let old = rustix::process::getrlimit(rustix::process::Resource::Nofile);
        rustix::process::setrlimit(
            rustix::process::Resource::Nofile,
            rustix::process::Rlimit {
                current: Some(64),
                maximum: old.maximum,
            },
        )?;
        let mut files = Vec::new();
        loop {
            match File::open("/dev/null") {
                Ok(file) => files.push(file),
                Err(error)
                    if error.raw_os_error() == Some(rustix::io::Errno::MFILE.raw_os_error()) =>
                {
                    break
                }
                Err(error) => return Err(error.into()),
            }
        }
        let error = match Pair::open(30, 100) {
            Ok(_) => {
                return Err("PTY allocation unexpectedly succeeded at exhausted fd limit".into())
            }
            Err(error) => error,
        };
        let errno = error.raw_os_error();
        drop(files);
        rustix::process::setrlimit(rustix::process::Resource::Nofile, old)?;
        let after = descriptors()?;
        if after != baseline {
            return Err("PTY allocation leaked descriptors".into());
        }
        println!(
            "{}",
            serde_json::json!({"errno":errno,"before":baseline,"after":after})
        );
    } else {
        let pair = Pair::open(30, 100)?;
        let result = CapturedCommand {
            launcher: PathBuf::from(input["launcher"].as_str().ok_or("launcher missing")?),
            cwd: std::env::current_dir()?,
            argv: vec!["/__owned_missing_pty_target__".into()],
        }
        .spawn_session_pty_with_env(pair.slave, &[]);
        let failed = result.is_err();
        let error = result.err().map(|error| error.to_string());
        drop(pair.master);
        let after = descriptors()?;
        let children =
            std::fs::read_to_string(format!("/proc/self/task/{}/children", std::process::id()))?;
        if !failed || after != baseline || !children.is_empty() {
            return Err("PTY exec failure did not close/reap".into());
        }
        println!(
            "{}",
            serde_json::json!({"failed_exec":failed,"error":error,"before":baseline,"after":after,"owned_children":children})
        );
    }
    Ok(())
}
