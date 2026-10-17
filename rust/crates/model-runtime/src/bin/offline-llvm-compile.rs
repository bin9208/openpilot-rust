//! One pinned LLVM library per process; all outputs are owned until complete.
use openpilot_model_runtime::compiler_backend_cpu::llvm::{self, Config};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
struct Outputs {
    paths: Vec<PathBuf>,
    committed: bool,
}
impl Drop for Outputs {
    fn drop(&mut self) {
        if !self.committed {
            for path in &self.paths {
                let _ = fs::remove_file(path);
            }
        }
    }
}
fn bounded_read(reader: impl Read, max: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("LLVM input exceeds length bound".into());
    }
    Ok(bytes)
}
fn run() -> Result<(), String> {
    // A worker is disposable: contain LLVM fatal errors, unbounded allocation
    // and code-generation CPU use without modifying the parent process limits.
    for (resource, limit) in [
        (libc::RLIMIT_CORE, 0),
        (libc::RLIMIT_AS, 4u64 << 30),
        (libc::RLIMIT_CPU, 60),
        (libc::RLIMIT_FSIZE, llvm::MAX_OUTPUT as u64),
    ] {
        let bounds = libc::rlimit {
            rlim_cur: limit,
            rlim_max: limit,
        };
        // SAFETY: setrlimit reads the provided live structure, retaining nothing.
        if unsafe { libc::setrlimit(resource, &bounds) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    let mut args = std::env::args_os().skip(1);
    let mut paths = Vec::new();
    for key in ["--config", "--object", "--receipt", "--optimized-ir"] {
        if args.next().as_deref() != Some(std::ffi::OsStr::new(key)) {
            return Err(format!("expected {key}"));
        }
        paths.push(PathBuf::from(
            args.next().ok_or_else(|| format!("{key} requires path"))?,
        ));
    }
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    // Canonicalize parents, not absent destinations; this catches aliases through
    // dot components/symlinks without resolving or modifying foreign files.
    let mut destinations = std::collections::BTreeSet::new();
    for path in &paths[1..] {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let key = fs::canonicalize(parent)
            .map_err(|e| e.to_string())?
            .join(path.file_name().ok_or("LLVM output requires file name")?);
        if !destinations.insert(key) {
            return Err("LLVM outputs alias".into());
        }
        if fs::symlink_metadata(path).is_ok() {
            return Err("LLVM output already exists".into());
        }
    }
    let config: Config = serde_json::from_slice(&bounded_read(
        File::open(&paths[0]).map_err(|e| e.to_string())?,
        65536,
    )?)
    .map_err(|e| e.to_string())?;
    let source = String::from_utf8(bounded_read(std::io::stdin(), llvm::MAX_SOURCE)?)
        .map_err(|e| e.to_string())?;
    // Disk guard precedes LLVM's possible large IR/object allocation and every
    // write. The estimate includes both bounded outputs and serialized receipt.
    for path in &paths[1..] {
        llvm::require_output_space(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
    }
    let result = llvm::compile_worker(&config, &source)?;
    let receipt = serde_json::to_vec(&result.receipt).map_err(|e| e.to_string())?;
    let mut transaction = Outputs {
        paths: Vec::new(),
        committed: false,
    };
    // create_new owns only paths created here; late I/O failure removes every
    // owned file, and never removes/replaces an existing caller-owned file.
    for (path, bytes) in [
        (&paths[1], result.object.as_slice()),
        (&paths[3], result.optimized_ir.as_bytes()),
        (&paths[2], receipt.as_slice()),
    ] {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        transaction.paths.push(path.clone());
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    transaction.committed = true;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("native LLVM compiler: {e}");
        std::process::exit(1)
    }
}
