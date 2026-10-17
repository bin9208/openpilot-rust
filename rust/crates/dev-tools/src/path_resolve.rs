//! Resolve paths with Python Path.resolve(strict=False) behavior.
use crate::seal::Result;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn resolve_non_strict(path: &Path) -> Result<PathBuf> {
    // Resolve links before consuming subsequent `..` components, even when a
    // preceding component is absent (Path.resolve(strict=False) semantics).
    fn resolve(
        path: &Path,
        mut resolved: PathBuf,
        active: &mut std::collections::HashSet<PathBuf>,
    ) -> Result<PathBuf> {
        use std::path::Component;
        for component in path.components() {
            match component {
                Component::RootDir => resolved = PathBuf::from("/"),
                Component::CurDir => {}
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::Normal(name) => {
                    resolved.push(name);
                    match fs::symlink_metadata(&resolved) {
                        Ok(metadata) if metadata.file_type().is_symlink() => {
                            let link = resolved.clone();
                            if !active.insert(link.clone()) {
                                return Err(format!("symlink loop: {}", link.display()).into());
                            }
                            let target = fs::read_link(&link)?;
                            resolved.pop();
                            resolved = resolve(&target, resolved, active)?;
                            active.remove(&link);
                        }
                        // Python's non-strict resolver tolerates unavailable and
                        // non-directory prefixes, but still rejects link loops.
                        _ => {}
                    }
                }
                Component::Prefix(_) => return Err("unsupported output path prefix".into()),
            }
        }
        Ok(resolved)
    }
    resolve(path, std::env::current_dir()?, &mut Default::default())
}
