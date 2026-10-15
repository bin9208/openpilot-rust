//! Verify the pinned detector archive without extracting its contents.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs,
    io::{Cursor, Read, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn relative(name: &[u8]) -> Result<Vec<u8>> {
    if name.starts_with(b"/") {
        return Err("absolute archive path".into());
    }
    let mut parts = name
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty() && *p != b".");
    if parts.next() != Some(b"charset-norm-3.5.1".as_slice()) {
        return Err("archive path outside pinned package".into());
    }
    let parts = parts.collect::<Vec<_>>();
    if parts.contains(&b"..".as_slice()) {
        return Err("parent archive path".into());
    }
    Ok(parts.join(&b'/'))
}
// tarfile's default UTF-8/surrogateescape filenames are byte-preserving on Unix.
fn filename(bytes: &[u8]) -> JsonValue {
    crate::python_fs::json_value(OsStr::from_bytes(bytes))
}
fn pretty(value: &JsonValue, depth: usize) -> Result<String> {
    enum Part {
        Node(JsonValue, usize),
        Text(String),
    }
    let mut pending = vec![Part::Node(value.clone(), depth)];
    let mut out = String::new();
    while let Some(part) = pending.pop() {
        let (value, depth) = match part {
            Part::Text(s) => {
                out.push_str(&s);
                continue;
            }
            Part::Node(v, d) => (v, d),
        };
        let (open, close, entries) = match value.view() {
            JsonView::Array(v) => (
                "[",
                "]",
                v.into_iter().map(|v| (None, v)).collect::<Vec<_>>(),
            ),
            JsonView::Object(v) => (
                "{",
                "}",
                v.into_iter().map(|(k, v)| (Some(k.to_vec()), v)).collect(),
            ),
            _ => {
                out.push_str(&value.to_json()?);
                continue;
            }
        };
        out.push_str(open);
        if entries.is_empty() {
            out.push_str(close);
            continue;
        }
        pending.push(Part::Text(format!("\n{}{close}", "  ".repeat(depth))));
        for (i, (key, value)) in entries.into_iter().enumerate().rev() {
            pending.push(Part::Node(value, depth + 1));
            let mut prefix = format!(
                "{}\n{}",
                if i == 0 { "" } else { "," },
                "  ".repeat(depth + 1)
            );
            if let Some(key) = key {
                prefix.push_str(
                    &JsonValue::codepoints(key)
                        .ok_or("invalid JSON key")?
                        .to_json()?,
                );
                prefix.push_str(": ");
            }
            pending.push(Part::Text(prefix));
        }
    }
    Ok(out)
}
fn replace_version(original: &[u8]) -> Vec<u8> {
    let old = b"rust-version = \"1.98\"";
    let new = b"rust-version = \"1.94\"";
    let mut out = vec![];
    let mut rest = original;
    while let Some(at) = rest.windows(old.len()).position(|value| value == old) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(new);
        rest = &rest[at + old.len()..];
    }
    out.extend_from_slice(rest);
    out
}
fn local_files(root: &Path) -> Result<BTreeSet<Vec<u8>>> {
    let mut result = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        // Python rglob suppresses errors opening an unreadable directory. Errors
        // raised while consuming an already-open directory iterator propagate.
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let ty = entry.file_type()?;
            if ty.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                result.insert(path.strip_prefix(root)?.as_os_str().as_bytes().to_vec());
            }
        }
    }
    Ok(result)
}
// Python tarfile treats a blank GNU sparse real_size as zero. tar 0.4
// requires an octal field. Normalize only an already checksum-validated empty
// sparse header, never its path, payload or the bytes used for archive identity.
fn empty_sparse_compat(data: &mut [u8]) -> Result<()> {
    let mut patches = vec![];
    {
        let mut archive = tar::Archive::new(Cursor::new(&*data));
        for entry in archive.entries()?.raw(true) {
            // Extended sparse records are not ordinary raw headers. The actual
            // parser below remains authoritative for every unmodified record.
            let Ok(entry) = entry else {
                break;
            };
            let header = entry.header();
            if !header.entry_type().is_gnu_sparse() || header.entry_size()? != 0 {
                continue;
            }
            if let Some(gnu) = header.as_gnu() {
                if gnu.realsize.iter().all(|b| *b == 0 || *b == b' ')
                    && !gnu.is_extended()
                    && gnu.sparse.iter().all(|v| v.is_empty())
                {
                    let mut header = header.clone();
                    header
                        .as_gnu_mut()
                        .ok_or("missing GNU header")?
                        .set_real_size(0);
                    header.set_cksum();
                    patches.push((usize::try_from(entry.raw_header_position())?, header));
                }
            }
        }
    }
    for (position, header) in patches {
        data[position..position + 512].copy_from_slice(header.as_bytes());
    }
    Ok(())
}
pub fn check(vendor: &Path, archive: &Path, output: &Path, printed: &mut dyn Write) -> Result<()> {
    let provenance = JsonValue::parse(&fs::read_to_string(
        vendor.join("charset-norm-provenance.json"),
    )?)?;
    let bytes = fs::read(archive)?;
    if !provenance
        .get("archive_sha256")
        .is_some_and(|value| value.text_eq(&hash(&bytes)))
    {
        return Err("vendor archive checksum mismatch".into());
    }
    let mut reader: Box<dyn Read> = if bytes.starts_with(&[0x1f, 0x8b]) {
        Box::new(flate2::read::MultiGzDecoder::new(Cursor::new(bytes)))
    } else if bytes.starts_with(b"BZh") {
        Box::new(bzip2::read::MultiBzDecoder::new(Cursor::new(bytes)))
    } else if bytes.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0]) {
        Box::new(xz2::read::XzDecoder::new_multi_decoder(Cursor::new(bytes)))
    } else {
        Box::new(Cursor::new(bytes))
    };
    let mut decoded = vec![];
    reader.read_to_end(&mut decoded)?;
    empty_sparse_compat(&mut decoded)?;
    let mut tar = tar::Archive::new(Cursor::new(decoded));
    let package = vendor.join("charset-norm");
    let mut files: Vec<(Vec<u8>, String)> = vec![];
    let mut changed = vec![];
    for entry in tar.entries()? {
        let mut entry = entry?;
        let relative = relative(&entry.path_bytes())?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            continue;
        }
        if !(kind.is_file() || kind.is_contiguous() || kind.is_gnu_sparse()) {
            return Err("archive member is not a regular file".into());
        }
        let mut original = vec![];
        entry.read_to_end(&mut original)?;
        let actual = fs::read(package.join(OsString::from_vec(relative.clone())))?;
        let expected = if relative == b"Cargo.toml" {
            replace_version(&original)
        } else {
            original.clone()
        };
        if actual != expected {
            return Err(format!(
                "vendor file differs: {}",
                String::from_utf8_lossy(&relative)
            )
            .into());
        }
        if actual != original {
            changed.push(relative.clone());
        }
        let digest = hash(&actual);
        if let Some((_, value)) = files.iter_mut().find(|(name, _)| *name == relative) {
            *value = digest;
        } else {
            files.push((relative, digest));
        }
    }
    if changed != [b"Cargo.toml".to_vec()] {
        return Err("expected only Cargo.toml rust-version to change".into());
    }
    if files
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>()
        != local_files(&package)?
    {
        return Err("vendor file set differs from archive".into());
    }
    let lines = files
        .iter()
        .map(|(name, digest)| {
            Ok(format!(
                "    {}: {}",
                filename(name).to_json()?,
                JsonValue::text(digest).to_json()?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let report=format!("{{\n  \"result\": \"PASS\",\n  \"provenance\": {},\n  \"changed_files\": [\n    \"Cargo.toml\"\n  ],\n  \"files\": {{\n{}\n  }}\n}}",pretty(&provenance,1)?,lines.join(",\n"));
    fs::write(output, report)?;
    writeln!(
        printed,
        "{} package files verified against published archive; only Cargo.toml rust-version changed",
        files.len()
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = crate::argparse_scan::scan_os_with_positionals(&args, &[], 2)?;
    if parsed.help {
        println!("Usage: check-registration-vendor ARCHIVE OUTPUT");
        return Ok(());
    }
    let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor");
    check(
        &vendor,
        Path::new(&parsed.positionals[0]),
        Path::new(&parsed.positionals[1]),
        &mut std::io::stdout().lock(),
    )
}
