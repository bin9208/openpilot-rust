//! Bounded source-only canonicalization; no compiler or model-provider loading.
use openpilot_model_runtime::{
    guarded_add::{canonicalize, MAX_RECEIPT_BYTES, MAX_SOURCE_BYTES},
    owned_outputs::Outputs,
};
use std::{
    ffi::OsStr,
    io::{Read, Write},
    path::PathBuf,
};

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new("--receipt")) {
        return Err("expected --receipt PATH".into());
    }
    let receipt_path = PathBuf::from(args.next().ok_or("--receipt requires path")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let mut source = Vec::new();
    std::io::stdin()
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut source)
        .map_err(|e| e.to_string())?;
    if source.len() > MAX_SOURCE_BYTES {
        return Err("LLVM source exceeds length bound".into());
    }
    let source = String::from_utf8(source).map_err(|e| e.to_string())?;
    let result = canonicalize(&source).map_err(|e| e.to_string())?;
    let receipt = serde_json::to_vec(&result.receipt).map_err(|e| e.to_string())?;
    if receipt.len() > MAX_RECEIPT_BYTES {
        return Err("rewrite receipt exceeds length bound".into());
    }
    let mut outputs = Outputs::default();
    outputs
        .write(&receipt_path, &receipt)
        .map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(result.source.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|e| e.to_string())?;
    outputs.commit().map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("native LLVM guarded add: {error}");
        std::process::exit(1);
    }
}
