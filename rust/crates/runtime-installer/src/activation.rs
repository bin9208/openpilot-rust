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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            cleanup_completed(root)?;
            // Also reap generations retained across the previous activation.
            if root.join(STORE).try_exists()? {
                clean_orphans(root)
            } else {
                Ok(())
            }
        }
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

// Keep the entire generation: Athena and other owners resolve sibling helpers
// from current_exe later, even when no helper process is running at activation.
fn live_generations(
    store: &Path,
    proc_root: &Path,
) -> std::io::Result<Option<std::collections::HashSet<PathBuf>>> {
    let mut live = std::collections::HashSet::new();
    for entry in fs::read_dir(proc_root)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_none_or(|name| name.parse::<u32>().is_err())
        {
            continue;
        }
        match fs::read_link(entry.path().join("exe")) {
            Ok(executable) => {
                use std::os::unix::ffi::OsStrExt;
                let bytes = executable.as_os_str().as_bytes();
                let executable = Path::new(std::ffi::OsStr::from_bytes(
                    bytes.strip_suffix(b" (deleted)").unwrap_or(bytes),
                ));
                if let Ok(relative) = executable.strip_prefix(store) {
                    if let Some(name) = relative.components().next() {
                        live.insert(store.join(name));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!(
                    "runtime generation cleanup deferred: cannot inspect live executable: {error}"
                );
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(Some(live))
}

fn clean_generations(root: &Path, active: &str) -> Result<()> {
    clean_generations_at(root, active, Path::new("/proc"))
}

fn clean_generations_at(root: &Path, active: &str, proc_root: &Path) -> Result<()> {
    let store = real_directory(root, Path::new(STORE))?;
    let store = fs::canonicalize(store)?;
    let live = match live_generations(&store, proc_root) {
        Ok(Some(live)) => live,
        Ok(None) => return Ok(()),
        Err(error) => {
            eprintln!("runtime generation cleanup deferred: executable inventory failed: {error}");
            return Ok(());
        }
    };
    for entry in fs::read_dir(&store)? {
        let entry = entry?;
        let name = entry.file_name();
        if !live.contains(&entry.path())
            && name
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn failed_process_inventory_defers_cleanup_without_failing_activation() {
        let root = tempfile::tempdir().unwrap();
        let name = format!("gen-{}-{}-old", "a".repeat(40), "b".repeat(64));
        let generation = root.path().join(STORE).join(name);
        fs::create_dir_all(&generation).unwrap();
        clean_generations_at(root.path(), "new", &root.path().join("missing-proc")).unwrap();
        assert!(generation.is_dir());
    }

    #[test]
    fn running_owner_retains_sibling_helpers_until_exit() {
        let root = tempfile::tempdir().unwrap();
        let name = format!("gen-{}-{}-old", "a".repeat(40), "b".repeat(64));
        let generation = root.path().join(STORE).join(&name);
        let bin = generation.join("rust/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::copy("/bin/sleep", bin.join("owner")).unwrap();
        fs::write(bin.join("future-helper"), b"required after activation").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut child = loop {
            match Command::new(bin.join("owner")).arg("30").spawn() {
                Ok(child) => break child,
                // Parallel subprocess fixtures can briefly inherit fs::copy's
                // writer before their exec closes it. Retry that fixture race
                // only; production executable ownership remains unchanged.
                Err(error)
                    if error.raw_os_error() == Some(rustix::io::Errno::TXTBSY.raw_os_error())
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("native owner fixture failed to start: {error}"),
            }
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while fs::read_link(format!("/proc/{}/exe", child.id())).unwrap() != bin.join("owner") {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let proc_view = tempfile::tempdir().unwrap();
        symlink(
            format!("/proc/{}", child.id()),
            proc_view.path().join(child.id().to_string()),
        )
        .unwrap();
        let result = clean_generations_at(root.path(), "new", proc_view.path());
        let retained = bin.join("future-helper").is_file();
        child.kill().unwrap();
        child.wait().unwrap();
        result.unwrap();
        assert!(
            retained,
            "running owner lost its generation and future sibling helper"
        );
        clean_generations_at(root.path(), "new", proc_view.path()).unwrap();
        assert!(!generation.exists());
    }
}
