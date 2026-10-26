use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub commit: String,
    pub tree: String,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()?;
    if !output.status.success() {
        return Err("cannot inspect runtime build source checkout".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim_end().to_owned())
}

pub fn inspect(root: &Path) -> Result<Source> {
    inspect_with_metadata(root, None)
}

/// Runtime metadata is the only generated file outside the ignored runtime tree.
/// Its exact bytes must be bound by the source-build descriptor. Tracked source
/// changes and every other untracked file remain errors.
pub(crate) fn inspect_with_metadata(root: &Path, metadata_sha256: Option<&str>) -> Result<Source> {
    inspect_runtime(root, metadata_sha256, &BTreeMap::new())
}

pub(crate) fn inspect_runtime(
    root: &Path,
    metadata_sha256: Option<&str>,
    assets: &BTreeMap<PathBuf, String>,
) -> Result<Source> {
    if Path::new(&git(root, &["rev-parse", "--show-toplevel"])?) != root {
        return Err("runtime build root must be the source checkout root".into());
    }
    let status = git(
        root,
        &[
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    for entry in status.split('\0').filter(|s| !s.is_empty()) {
        if metadata_sha256.is_some() && entry == "?? build.json" {
            continue;
        }
        if let Some(path) = entry.strip_prefix(" M ") {
            if let Some(expected) = assets.get(Path::new(path)) {
                if lfs_identity(root, Path::new(path))?.is_some_and(|(sha, bytes)| {
                    sha == *expected
                        && std::fs::metadata(root.join(path)).is_ok_and(|m| m.len() == bytes)
                }) && crate::digest_file(&root.join(path))? == *expected
                {
                    continue;
                }
            }
        }
        return Err(
            "runtime source differs from its committed source or exact LFS asset identities".into(),
        );
    }
    for (path, expected) in assets {
        if !openpilot_runtime_installer::source_asset_allowed(path) {
            return Err("runtime source override is not an allowed asset".into());
        }
        if crate::regular_file(&root.join(path))?
            .metadata()?
            .permissions()
            .mode()
            & 0o7111
            != 0
        {
            return Err("runtime source asset has executable or special permission bits".into());
        }
        if let Some((sha, bytes)) = lfs_identity(root, path)? {
            if sha != *expected
                || crate::regular_file(&root.join(path))?.metadata()?.len() != bytes
                || crate::digest_file(&root.join(path))? != sha
            {
                return Err("runtime LFS source asset does not match the committed object".into());
            }
        }
    }
    if let Some(expected) = metadata_sha256 {
        if crate::digest_file(&root.join("build.json"))? != expected {
            return Err("source-build metadata digest mismatch".into());
        }
    }
    let commit = git(root, &["rev-parse", "--verify", "HEAD"])?;
    let tree = git(root, &["rev-parse", "--verify", "HEAD^{tree}"])?;
    if [commit.as_str(), tree.as_str()].iter().any(|value| {
        value.len() != 40
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) {
        return Err("runtime build source requires full Git SHA-1 identities".into());
    }
    Ok(Source { commit, tree })
}

/// Only the exact committed Git LFS object can replace a source pointer.
pub(crate) fn lfs_identity(root: &Path, path: &Path) -> Result<Option<(String, u64)>> {
    if !openpilot_runtime_installer::source_asset_allowed(path) {
        return Err("unexpected source asset".into());
    }
    let object = format!("HEAD:{}", path.to_str().ok_or("invalid source asset path")?);
    let size = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "-s", &object])
        .output()?;
    // Generated firmware may be absent from the committed tree.
    if !size.status.success() {
        return Ok(None);
    }
    let size = String::from_utf8(size.stdout)?.trim().parse::<u64>()?;
    if size > 1024 {
        return Ok(None);
    }
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "blob", &object])
        .output()?;
    if !output.status.success() || output.stdout.len() as u64 != size {
        return Err("cannot read committed source asset identity".into());
    }
    let Ok(pointer) = std::str::from_utf8(&output.stdout) else {
        return Ok(None);
    };
    if !pointer.starts_with("version https://git-lfs.github.com/spec/v1\n") {
        return Ok(None);
    }
    let lines: Vec<_> = pointer.lines().collect();
    if lines.len() != 3 {
        return Err("invalid committed LFS pointer".into());
    }
    let sha = lines[1]
        .strip_prefix("oid sha256:")
        .ok_or("invalid LFS digest")?;
    let bytes = lines[2]
        .strip_prefix("size ")
        .ok_or("invalid LFS size")?
        .parse::<u64>()?;
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || bytes == 0
    {
        return Err("invalid committed LFS object identity".into());
    }
    Ok(Some((sha.into(), bytes)))
}
