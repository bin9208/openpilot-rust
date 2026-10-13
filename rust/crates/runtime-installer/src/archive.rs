use crate::{
    identity::{self, Descriptor, MAX_JSON_BYTES},
    invalid, Result,
};
use flate2::read::MultiGzDecoder;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Component, Path, PathBuf},
};

pub(crate) fn real_directory(root: &Path, relative: &Path) -> Result<PathBuf> {
    if !identity::clean_path(relative) {
        return Err(invalid("invalid directory path"));
    }
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => (),
            Ok(_) => {
                return Err(invalid(format!(
                    "directory is not a real directory: {}",
                    path.display()
                )))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&path) {
                    Ok(()) => (),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(error) => return Err(error.into()),
                }
                if !fs::symlink_metadata(&path)?.is_dir() {
                    return Err(invalid("concurrently created path is not a real directory"));
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}

#[derive(Default)]
struct Extensions {
    path: Option<PathBuf>,
    link: Option<PathBuf>,
}

pub(crate) fn extract(archive: &Path, root: &Path, descriptor: &Descriptor) -> Result<()> {
    let overhead = 32 * 1024 * 1024;
    let limit = descriptor
        .archive
        .unpacked_bytes
        .checked_add(overhead)
        .ok_or_else(|| invalid("archive size overflow"))?;
    let decoder = MultiGzDecoder::new(File::open(archive)?);
    let mut archive = tar::Archive::new(decoder.take(limit));
    let mut seen = BTreeSet::new();
    let mut links = Vec::new();
    let mut extensions = Extensions::default();
    let mut bytes = 0_u64;
    for entry in archive.entries()?.raw(true) {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_gnu_longname() || kind.is_gnu_longlink() || kind.is_pax_local_extensions() {
            extension(&mut entry, &mut extensions)?;
            continue;
        }
        let path = match extensions.path.take() {
            Some(p) => p,
            None => entry.path()?.into_owned(),
        };
        if !identity::clean_path(&path) || !seen.insert(path.clone()) || seen.len() > 20000 {
            return Err(invalid("invalid or duplicate archive member"));
        }
        let allowed =
            identity::allowed_artifact(&path) || path == Path::new("rust/runtime-manifest.json");
        if kind.is_dir() {
            if !directory_allowed(&path) || entry.size() != 0 {
                return Err(invalid("unexpected archive directory"));
            }
            real_directory(root, &path)?;
        } else if kind.is_file() && allowed {
            let size = entry.size();
            bytes = bytes
                .checked_add(size)
                .ok_or_else(|| invalid("archive bytes overflow"))?;
            if bytes > descriptor.archive.unpacked_bytes
                || (path == Path::new("rust/runtime-manifest.json") && size > MAX_JSON_BYTES)
            {
                return Err(invalid("archive exceeds unpacked byte bound"));
            }
            let mode = entry.header().mode()?;
            if mode & 0o6000 != 0 {
                return Err(invalid("privileged archive mode forbidden"));
            }
            let parent = path
                .parent()
                .ok_or_else(|| invalid("member has no parent"))?;
            real_directory(root, parent)?;
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(root.join(&path))?;
            if std::io::copy(&mut entry, &mut file)? != size {
                return Err(invalid("truncated archive member"));
            }
            file.flush()?;
            file.set_permissions(fs::Permissions::from_mode(mode & 0o777))?;
            file.sync_all()?;
        } else if kind.is_symlink() && allowed && path.starts_with("rust/bin") {
            if entry.size() != 0 {
                return Err(invalid("symlink carries data"));
            }
            let target = match extensions.link.take() {
                Some(p) => p,
                None => entry
                    .link_name()?
                    .ok_or_else(|| invalid("symlink has no target"))?
                    .into_owned(),
            };
            let destination = link_destination(&path, &target)?;
            links.push((path, target, destination));
        } else {
            return Err(invalid("unexpected archive member type or path"));
        }
        if extensions.link.is_some() {
            return Err(invalid("unused archive link extension"));
        }
    }
    if extensions.path.is_some()
        || extensions.link.is_some()
        || bytes != descriptor.archive.unpacked_bytes
    {
        return Err(invalid("incomplete archive or unpacked size mismatch"));
    }
    let mut tail = archive.into_inner();
    let mut buffer = [0; 65536];
    loop {
        let count = tail.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if buffer[..count].iter().any(|byte| *byte != 0) {
            return Err(invalid("unexpected data after tar end marker"));
        }
    }
    if tail.limit() == 0 {
        return Err(invalid("archive exceeds total decompressed limit"));
    }
    for (path, target, destination) in &links {
        if !seen.contains(destination) {
            return Err(invalid("link target absent from archive"));
        }
        let parent = path.parent().ok_or_else(|| invalid("link has no parent"))?;
        real_directory(root, parent)?;
        symlink(target, root.join(path))?;
    }
    for (path, _, _) in links {
        let resolved = root.join(path).canonicalize()?;
        if !resolved.starts_with(root.join("rust/bin")) || !resolved.is_file() {
            return Err(invalid("symlink does not resolve to internal regular file"));
        }
    }
    Ok(())
}

fn directory_allowed(path: &Path) -> bool {
    Path::new("rust/bin").starts_with(path)
        || path.starts_with("rust/bin")
        || identity::SOURCE_ASSETS.iter().any(|asset| {
            Path::new(asset)
                .parent()
                .is_some_and(|parent| parent.starts_with(path))
        })
}

fn link_destination(path: &Path, target: &Path) -> Result<PathBuf> {
    if target.is_absolute()
        || target.as_os_str().is_empty()
        || target.as_os_str().len() > 4096
        || target.to_str().is_none_or(|s| s.contains('\\'))
    {
        return Err(invalid("invalid link target"));
    }
    let mut normalized = path
        .parent()
        .ok_or_else(|| invalid("link has no parent"))?
        .to_path_buf();
    for part in target.components() {
        match part {
            Component::Normal(p) => normalized.push(p),
            Component::CurDir => (),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(invalid("link escapes package"));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(invalid("absolute link forbidden"))
            }
        }
    }
    if !normalized.starts_with("rust/bin") || normalized == Path::new("rust/bin") {
        return Err(invalid("link escapes runtime binaries"));
    }
    Ok(normalized)
}

fn extension<R: Read>(entry: &mut tar::Entry<'_, R>, extensions: &mut Extensions) -> Result<()> {
    if entry.size() > 4096 {
        return Err(invalid("tar extension exceeds bound"));
    }
    let kind = entry.header().entry_type();
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    if kind.is_gnu_longname() || kind.is_gnu_longlink() {
        if bytes.last() == Some(&0) {
            bytes.pop();
        }
        let path =
            PathBuf::from(String::from_utf8(bytes).map_err(|_| invalid("non-UTF8 tar extension"))?);
        let slot = if kind.is_gnu_longname() {
            &mut extensions.path
        } else {
            &mut extensions.link
        };
        if slot.replace(path).is_some() {
            return Err(invalid("duplicate tar extension"));
        }
    } else {
        let mut remainder = bytes.as_slice();
        while !remainder.is_empty() {
            let space = remainder
                .iter()
                .position(|b| *b == b' ')
                .ok_or_else(|| invalid("malformed PAX length"))?;
            let size: usize = std::str::from_utf8(&remainder[..space])
                .map_err(|_| invalid("invalid PAX length"))?
                .parse()
                .map_err(|_| invalid("invalid PAX length"))?;
            if size <= space + 1 || size > remainder.len() || remainder[size - 1] != b'\n' {
                return Err(invalid("invalid PAX record"));
            }
            let record = std::str::from_utf8(&remainder[space + 1..size - 1])
                .map_err(|_| invalid("non-UTF8 PAX record"))?;
            let (key, value) = record
                .split_once('=')
                .ok_or_else(|| invalid("invalid PAX key"))?;
            match key {
                "path" => {
                    if extensions.path.replace(PathBuf::from(value)).is_some() {
                        return Err(invalid("duplicate PAX path"));
                    }
                }
                "linkpath" => {
                    if extensions.link.replace(PathBuf::from(value)).is_some() {
                        return Err(invalid("duplicate PAX link"));
                    }
                }
                "mtime" | "atime" | "ctime" => (),
                _ => return Err(invalid("unsupported PAX field")),
            }
            remainder = &remainder[size..];
        }
    }
    Ok(())
}
