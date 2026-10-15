//! Harmless native commands used by the updater subprocess differential checker.
use crate::seal::Result;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::Command,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
pub fn source_sha256() -> String {
    format!(
        "{:x}",
        Sha256::digest(include_bytes!("updated_process_fixture.rs"))
    )
}
pub fn cli() -> Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [mode] if mode=="identity"=>println!("{}",serde_json::json!({"schema":"updated-process-fixture-v1","source_sha256":source_sha256()})),
        [mode] if mode=="merged-output"=>{
            io::stdout().write_all(b"one\r\n")?;io::stdout().flush()?;
            io::stderr().write_all(b"two\r")?;io::stderr().flush()?;
        },
        [mode] if mode=="nonzero"=>{println!("failure detail");std::process::exit(7);},
        [mode] if mode=="environment"=>{
            for key in ["test.preserved","gc.auto","gc.autoDetach","maintenance.auto"] {
                let status=Command::new("git").args(["config","--get",key]).status()?;
                if !status.success(){return Err(format!("git config {key} failed: {status}").into());}
            }
        },
        [mode,pid_file,parent_exits] if mode=="group-leader"=>{
            let parent_exits=parent_exits.parse::<bool>()?;
            // Deliberately leave the worker and its inherited output pipe owned by
            // the updater command's session, even after this direct leader exits.
            let _worker=Command::new(std::env::current_exe()?).arg("worker").arg(pid_file).spawn()?;
            if !parent_exits {std::thread::sleep(Duration::from_secs(60));}
        },
        [mode,pid_file] if mode=="worker"=>{
            let ignored=Arc::new(AtomicBool::new(false));
            let _int=signal_hook::flag::register(signal_hook::consts::SIGINT,Arc::clone(&ignored))?;
            let _term=signal_hook::flag::register(signal_hook::consts::SIGTERM,ignored)?;
            let group=rustix::process::getpgrp().as_raw_nonzero().get();
            fs::write(format!("{pid_file}.group"),group.to_string())?;
            fs::write(Path::new(pid_file),std::process::id().to_string())?;
            std::thread::sleep(Duration::from_secs(60));
        },
        _=>return Err("expected identity, merged-output, nonzero, environment, group-leader PID_FILE BOOL, or worker PID_FILE".into()),
    }
    Ok(())
}
