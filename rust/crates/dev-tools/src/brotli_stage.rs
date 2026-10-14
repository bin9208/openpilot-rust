//! Stage trusted external SDK Brotli providers; codec libraries stay external.
use crate::{disk_space, seal::Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path};

const LIBRARIES: [&str; 3] = [
    "libbrotlienc.so.1",
    "libbrotlidec.so.1",
    "libbrotlicommon.so.1",
];
const MAX_LIBRARY: u64 = 16 * 1024 * 1024;

// Struct fields retain Python's declared byte order with either serde_json map mode.
#[derive(Debug, Serialize)]
struct Files {
    #[serde(rename = "libbrotlienc.so.1")]
    encoder: String,
    #[serde(rename = "libbrotlidec.so.1")]
    decoder: String,
    #[serde(rename = "libbrotlicommon.so.1")]
    common: String,
}
#[derive(Debug, Serialize)]
struct Manifest {
    abi_version: u32,
    target: String,
    brotli_version: u32,
    files: Files,
}
#[derive(Debug, Serialize)]
pub struct Report {
    bundle: String,
    manifest: Manifest,
    bytes: u64,
}

fn version(value: &str) -> Result<u32> {
    let parts = value
        .split('.')
        .map(str::trim)
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    match parts.as_slice() {
        [1, minor, patch] if *minor < 4096 && *patch < 4096 => {
            Ok((1 << 24) | (minor << 12) | patch)
        }
        _ => Err("unsupported Brotli version".into()),
    }
}

pub fn stage(source: &Path, output: &Path, target: &str, declared_version: &str) -> Result<Report> {
    let machine: u16 = match target {
        "x86_64-unknown-linux-gnu" => 62,
        "aarch64-unknown-linux-gnu" => 183,
        _ => return Err("unsupported Brotli target".into()),
    };
    let brotli_version = version(declared_version)?;
    let source = source.canonicalize()?;
    if !source.is_dir() {
        return Err("SDK source is not a directory".into());
    }
    let output = std::env::current_dir()?.join(output);
    match output.symlink_metadata() {
        Ok(_) => return Err("Brotli bundle destination already exists".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let parent = output.parent().ok_or("Brotli destination parent missing")?;
    let mut inputs = Vec::with_capacity(3);
    let mut bytes = 0_u64;
    for name in LIBRARIES {
        let path = source.join(name).canonicalize()?;
        if !path.starts_with(&source) {
            return Err(format!("SDK library escapes the source directory: {name}").into());
        }
        // Nonblocking avoids waiting on a FIFO substituted for a library. Resolve
        // SDK symlinks above, but do not follow a newly substituted leaf symlink.
        let flags =
            i32::try_from((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits())?;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(flags)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(format!("SDK library is not regular: {name}").into());
        }
        if metadata.len() > MAX_LIBRARY {
            return Err(format!("SDK library size exceeds 16 MiB: {name}").into());
        }
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or("Brotli bundle size overflow")?;
        inputs.push((name, file, metadata.len()));
    }
    let growth = bytes
        .checked_add(1024 * 1024)
        .ok_or("Brotli staging growth overflow")?;
    disk_space::inspect(parent, growth)?.require()?;
    let mut files = Vec::with_capacity(3);
    for (name, file, length) in inputs {
        let mut raw = Vec::with_capacity(usize::try_from(length)?);
        file.take(MAX_LIBRARY + 1).read_to_end(&mut raw)?;
        if u64::try_from(raw.len())? != length {
            return Err(format!("SDK library size changed while reading: {name}").into());
        }
        // The original contract checks class, endianness, ET_DYN and e_machine.
        // Missing high bytes retain int.from_bytes(slice, 'little') semantics.
        let word = |offset: usize| {
            u16::from_le_bytes([
                raw.get(offset).copied().unwrap_or(0),
                raw.get(offset + 1).copied().unwrap_or(0),
            ])
        };
        if raw.get(..6) != Some(b"\x7fELF\x02\x01".as_slice())
            || word(16) != 3
            || word(18) != machine
        {
            return Err(format!("ELF target mismatch: {name}").into());
        }
        files.push((name, raw));
    }
    let hashes: Vec<_> = files
        .iter()
        .map(|(_, raw)| format!("{:x}", Sha256::digest(raw)))
        .collect();
    let manifest = Manifest {
        abi_version: 1,
        target: target.to_owned(),
        brotli_version,
        files: Files {
            encoder: hashes[0].clone(),
            decoder: hashes[1].clone(),
            common: hashes[2].clone(),
        },
    };
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    manifest_bytes.push(b'\n');
    fs::create_dir_all(parent)?;
    let bundle = parent
        .canonicalize()?
        .join(
            output
                .file_name()
                .ok_or("Brotli destination name missing")?,
        )
        .into_os_string()
        .into_string()
        .map_err(|_| "Brotli destination is not UTF-8")?;
    let temporary = tempfile::Builder::new()
        .prefix(".brotli-")
        .tempdir_in(parent)?;
    for (name, raw) in files {
        fs::write(temporary.path().join(name), raw)?;
    }
    fs::write(temporary.path().join("manifest.json"), manifest_bytes)?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temporary.path(),
        rustix::fs::CWD,
        &output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    Ok(Report {
        bundle,
        manifest,
        bytes,
    })
}
