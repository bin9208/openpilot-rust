use crate::{
    archive::real_directory,
    copy_bounded,
    identity::{self, Descriptor},
    invalid, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};

const JOURNAL: &str = "rust/.runtime-install-journal.json";
const STORE: &str = "rust/.runtime-packages";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    generation: String,
}

fn valid_name(name: &str) -> bool {
    let mut parts = name.split('-');
    matches!((parts.next(),parts.next(),parts.next(),parts.next(),parts.next()),(Some("gen"),Some(sha),Some(hash),Some(nonce),None) if identity::hex(sha,40) && identity::hex(hash,64) && !nonce.is_empty() && nonce.len() <= 32 && nonce.bytes().all(|b| b.is_ascii_alphanumeric()))
}

pub(crate) fn active_generation(root: &Path) -> Result<PathBuf> {
    let link = fs::read_link(root.join("rust/.runtime-active"))?;
    let name = link
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("invalid active generation link"))?;
    if !valid_name(name) || link != Path::new(".runtime-packages").join(name) {
        return Err(invalid("active generation is outside installer store"));
    }
    let store = real_directory(root, Path::new(STORE))?;
    let generation = store.join(name);
    if !fs::symlink_metadata(&generation)?.is_dir() {
        return Err(invalid("generation is not a real directory"));
    }
    Ok(generation)
}

pub(crate) fn activate(
    root: &Path,
    staging: tempfile::TempDir,
    descriptor: &Descriptor,
) -> Result<()> {
    let suffix = staging
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix(".pending-"))
        .ok_or_else(|| invalid("invalid staging directory"))?;
    let name = format!(
        "gen-{}-{}-{suffix}",
        descriptor.source_commit, descriptor.archive.sha256
    );
    if !valid_name(&name) {
        return Err(invalid("invalid generation name"));
    }
    let destination = root.join(STORE).join(&name);
    fs::rename(staging.path(), &destination)?;
    sync_tree(&destination)?;
    sync_directory(&root.join(STORE))?;
    let journal = Journal { generation: name };
    write_json_atomic(root, &journal)?;
    finish(root, &journal)?;
    clean_generations(root, &journal.generation)
}

pub(crate) fn recover(root: &Path) -> Result<()> {
    let path = root.join(JOURNAL);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => {
            let journal: Journal = identity::read_json(&path)?;
            finish(root, &journal)?;
            clean_generations(root, &journal.generation)
        }
        Ok(_) => Err(invalid("activation journal is not a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => cleanup_completed(root),
        Err(e) => Err(e.into()),
    }
}

fn cleanup_completed(root: &Path) -> Result<()> {
    let rollback = root.join("rust/.runtime-install-rollback");
    match fs::symlink_metadata(&rollback) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Ok(metadata) if metadata.is_dir() => (),
        Ok(_) => return Err(invalid("activation backup is not a real directory")),
        Err(e) => return Err(e.into()),
    }
    let generation = active_generation(root)?;
    let descriptor: Descriptor = identity::read_json(&generation.join(".runtime-release.json"))?;
    descriptor.validate(&descriptor.source_commit)?;
    let manifest = identity::verify_generation(&generation, &descriptor)?;
    identity::verify_files(root, &manifest)?;
    for entry in fs::read_dir(&rollback)? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|s| s == "bin" || s == "manifest.json" || s.starts_with(".source-"))
        {
            return Err(invalid(
                "unknown file in activation backup; refusing removal",
            ));
        }
    }
    fs::remove_dir_all(rollback)?;
    sync_directory(&root.join("rust"))
}

fn sync_tree(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            }
        }
        directories.push(directory);
    }
    for directory in directories.iter().rev() {
        sync_directory(directory)?;
    }
    Ok(())
}

fn finish(root: &Path, journal: &Journal) -> Result<()> {
    if !valid_name(&journal.generation) {
        return Err(invalid("invalid journal generation"));
    }
    let store = real_directory(root, Path::new(STORE))?;
    let generation = store.join(&journal.generation);
    if !fs::symlink_metadata(&generation)?.is_dir() {
        return Err(invalid("journal generation is not a real directory"));
    }
    let descriptor: Descriptor = identity::read_json(&generation.join(".runtime-release.json"))?;
    descriptor.validate(&descriptor.source_commit)?;
    let manifest = identity::verify_generation(&generation, &descriptor)?;
    let rollback = real_directory(root, Path::new("rust/.runtime-install-rollback"))?;
    clean_source_temps(&rollback)?;
    for artifact in manifest
        .files
        .iter()
        .filter(|f| identity::source_asset(&f.path))
    {
        let parent = artifact
            .path
            .parent()
            .ok_or_else(|| invalid("source asset has no parent"))?;
        real_directory(root, parent)?;
        let target = root.join(&artifact.path);
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() => {
                if identity::digest(&target)? == artifact.sha256 {
                    continue;
                }
            }
            Ok(_) => return Err(invalid("source asset target is not a regular file")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        let source = generation.join(&artifact.path);
        let temporary = tempfile::Builder::new()
            .prefix(".source-")
            .tempfile_in(&rollback)?;
        copy_bounded(&source, temporary.path(), fs::metadata(&source)?.len())?;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(
                fs::metadata(&source)?.permissions().mode() & 0o777,
            ))?;
        temporary.as_file().sync_all()?;
        temporary.persist(&target).map_err(|e| e.error)?;
        sync_directory(&root.join(parent))?;
    }
    for (path, target, backup) in [
        ("rust/bin", ".runtime-active/rust/bin", "bin"),
        (
            "rust/runtime-manifest.json",
            ".runtime-active/rust/runtime-manifest.json",
            "manifest.json",
        ),
    ] {
        let path = root.join(path);
        if fs::read_link(&path).is_ok_and(|existing| existing == Path::new(target)) {
            continue;
        }
        if fs::symlink_metadata(&path).is_ok() {
            if fs::symlink_metadata(rollback.join(backup)).is_ok() {
                return Err(invalid("conflicting legacy runtime and activation backup"));
            }
            fs::rename(&path, rollback.join(backup))?;
            sync_directory(&rollback)?;
            sync_directory(&root.join("rust"))?;
        }
        symlink(target, &path)?;
        sync_directory(&root.join("rust"))?;
    }
    replace_link(
        &root.join("rust/.runtime-active"),
        &Path::new(".runtime-packages").join(&journal.generation),
    )?;
    // Check all source paths and the installed bin view before releasing the journal.
    identity::verify_files(root, &manifest)?;
    fs::remove_file(root.join(JOURNAL))?;
    sync_directory(&root.join("rust"))?;
    fs::remove_dir_all(&rollback)?;
    sync_directory(&root.join("rust"))
}

fn replace_link(path: &Path, target: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("activation path has no parent"))?;
    let temporary = tempfile::Builder::new()
        .prefix(".runtime-link-")
        .tempdir_in(parent)?;
    let link = temporary.path().join("link");
    symlink(target, &link)?;
    fs::rename(&link, path)?;
    sync_directory(parent)
}

fn write_json_atomic(root: &Path, journal: &Journal) -> Result<()> {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(root.join("rust"))?;
    file.write_all(&serde_json::to_vec(journal)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join(JOURNAL)).map_err(|e| e.error)?;
    sync_directory(&root.join("rust"))
}

fn clean_source_temps(rollback: &Path) -> Result<()> {
    for entry in fs::read_dir(rollback)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|s| s.starts_with(".source-"))
            && entry.file_type()?.is_file()
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

pub(crate) fn clean_pending(store: &Path) -> Result<()> {
    for entry in fs::read_dir(store)? {
        let entry = entry?;
        let name = entry.file_name();
        let owned = name.to_str().is_some_and(|s| {
            s.strip_prefix(".pending-").is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix.len() <= 32
                    && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
            })
        });
        if owned && entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn clean_generations(root: &Path, active: &str) -> Result<()> {
    let store = real_directory(root, Path::new(STORE))?;
    for entry in fs::read_dir(&store)? {
        let entry = entry?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|name| valid_name(name) && name != active)
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    sync_directory(&store)
}

pub(crate) fn clean_orphans(root: &Path) -> Result<()> {
    let active = match fs::read_link(root.join("rust/.runtime-active")) {
        Ok(link) => {
            let name = link
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| invalid("invalid active generation link"))?;
            if !valid_name(name) || link != Path::new(".runtime-packages").join(name) {
                return Err(invalid("invalid active generation link"));
            }
            name.to_owned()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    clean_generations(root, &active)
}
