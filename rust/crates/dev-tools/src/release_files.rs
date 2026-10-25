//! Original release-tree selection. Regex spelling is intentionally unchanged.
use regex::Regex;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::LazyLock,
};

static BLACKLIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r".git/|.github/workflows/|matlab.*.md|.lfsconfig|.gitattributes|.git$|.gitmodules")
        .expect("fixed release blacklist is valid")
});

pub fn list(root: &Path) -> io::Result<Vec<PathBuf>> {
    fn walk(root: &Path, directory: &Path, result: &mut Vec<PathBuf>) -> io::Result<()> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            // Path.walk/rglob suppress failures opening a directory. Errors
            // while iterating an already opened directory still propagate.
            Err(_) => return Ok(()),
        };
        let mut directories = Vec::new();
        for entry in entries {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                directories.push(path);
            } else if kind.is_file() || kind.is_symlink() {
                let relative = path
                    .strip_prefix(root)
                    .expect("entry is inside supplied root");
                if !BLACKLIST.is_match(&relative.to_string_lossy()) {
                    result.push(relative.to_owned());
                }
            }
        }
        // Directory symlinks are selected as links, never traversed, matching
        // pathlib's original recursive glob (including dangling links/loops).
        for directory in directories {
            walk(root, &directory, result)?;
        }
        Ok(())
    }
    let mut result = Vec::new();
    walk(root, root, &mut result)?;
    Ok(result)
}
