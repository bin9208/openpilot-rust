//! Candidate identity includes dirty native sources; a Git revision is provenance only.
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const PROTOCOL: &str = "dev-md-native-radar-replay-v1";
pub const UPSTREAM: &str = "e8e152dbcfd9574a1791ed12026d1880e87689b3";

fn collect(
    root: &Path,
    relative: &Path,
    files: &mut Vec<PathBuf>,
    native_only: bool,
) -> io::Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(root, &path, files, native_only)?;
        } else if kind.is_file()
            && (!native_only
                || matches!(
                    path.extension().and_then(|v| v.to_str()),
                    Some(
                        "rs" | "toml"
                            | "lock"
                            | "capnp"
                            | "dbc"
                            | "json"
                            | "c"
                            | "h"
                            | "cc"
                            | "cpp"
                            | "cl"
                            | "wgsl"
                    )
                ))
        {
            files.push(path);
        }
    }
    Ok(())
}

pub fn fingerprint(root: &Path) -> io::Result<String> {
    let mut files = vec![
        PathBuf::from("rust/Cargo.toml"),
        PathBuf::from("rust/Cargo.lock"),
        PathBuf::from("rust/upstream/dbc-generators.json"),
        PathBuf::from("rust/upstream/radar-final-source.json"),
        PathBuf::from("rust/upstream/planner-source.json"),
        PathBuf::from("rust/upstream/signal-source.json"),
        PathBuf::from("rust/upstream/signal-runtime.json"),
    ];
    for dir in [
        "rust/crates",
        "openpilot/cereal",
        "opendbc_repo/opendbc/dbc",
        "opendbc_repo/opendbc/car",
    ] {
        collect(root, Path::new(dir), &mut files, true)?;
    }
    files.sort();
    files.dedup();
    let mut hash = Sha256::new();
    hash.update(PROTOCOL);
    hash.update(UPSTREAM);
    for path in files {
        let name = path.to_string_lossy();
        let bytes = fs::read(root.join(&path))?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn file_hash(path: &Path) -> io::Result<String> {
    use io::Read;
    let mut input = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut block = [0; 65536];
    loop {
        let n = input.read(&mut block)?;
        if n == 0 {
            break;
        }
        hash.update(&block[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Offline asset identity includes bytes, names and boundaries in stable order.
pub fn directory_hash(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect(root, Path::new(""), &mut files, false)?;
    files.sort();
    let mut hash = Sha256::new();
    for file in files {
        let name = file.to_string_lossy();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(file_hash(&root.join(file))?);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Receipts bind options and actual providers before any cache can be reused.
pub fn cache_key(receipt: &serde_json::Value) -> io::Result<String> {
    let bytes = serde_json::to_vec(receipt)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
