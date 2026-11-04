//! Mandatory committed source-object checks, also available to consumers built
//! without archive/download preparation features.
use crate::{digest_file, objects::Object, regular_file, source, Result};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

pub(crate) fn identities(root: &Path) -> Result<BTreeMap<PathBuf, (String, u64)>> {
    let mut result = BTreeMap::new();
    for path in openpilot_runtime_installer::source_asset_paths() {
        let path = Path::new(path);
        if let Some(identity) = source::lfs_identity(root, path)? {
            result.insert(path.to_owned(), identity);
        }
    }
    Ok(result)
}

/// Inspect required mappings before expensive compilation and again at seal.
/// The full provider Tree/receipt is independently bound by the build plan.
pub fn verify_layout(
    source_root: &Path,
    provider_root: &Path,
    files: &BTreeMap<PathBuf, PathBuf>,
) -> Result<()> {
    let source_root = source_root.canonicalize()?;
    let before = source::inspect(&source_root)?;
    let expected = identities(&source_root)?;
    if !expected.is_empty() {
        let provider_root = provider_root.canonicalize()?;
        for (path, (sha, bytes)) in expected {
            let input = files
                .get(&path)
                .ok_or("native layout is missing a committed source LFS object")?;
            if input.as_os_str().is_empty()
                || !input
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
            {
                return Err("unsafe committed source LFS mapping".into());
            }
            let input = provider_root.join(input).canonicalize()?;
            if !input.starts_with(&provider_root) {
                return Err("committed source LFS mapping escapes its provider".into());
            }
            let metadata = regular_file(&input)?.metadata()?;
            if metadata.len() != bytes
                || metadata.permissions().mode() & 0o7777 != 0o444
                || digest_file(&input)? != sha
            {
                return Err("native source LFS mapping differs from its committed object".into());
            }
        }
    }
    if source::inspect(&source_root)? != before {
        return Err("source changed during LFS mapping verification".into());
    }
    Ok(())
}

/// Metadata-only roster gate for the actual sealed objects. Existing runtime,
/// manifest and object-store verification still checks their bytes and modes.
/// This gate needs no compiler/provider caches at startup or fallback selection.
pub fn verify_recorded(source_root: &Path, files: &BTreeMap<PathBuf, Object>) -> Result<()> {
    for (path, (sha, bytes)) in identities(source_root)? {
        let object = files
            .get(&path)
            .ok_or("sealed runtime omitted a committed source LFS object")?;
        if object.sha256 != sha || object.bytes != bytes || object.mode != 0o444 {
            return Err("sealed source LFS object differs from its committed identity".into());
        }
    }
    Ok(())
}
