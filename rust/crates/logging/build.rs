use std::{
    collections::BTreeSet,
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

fn add_watch(path: &Path, output: &Path, watches: &mut BTreeSet<PathBuf>) -> io::Result<()> {
    let canonical = path.canonicalize()?;
    let metadata = fs::metadata(&canonical)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "build watch is not a regular file or directory",
        ));
    }
    if metadata.is_dir() && output.starts_with(&canonical) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "build watch would recursively include Cargo OUT_DIR",
        ));
    }
    // Cargo's directory walk observes the original directory symlink's mtime.
    watches.insert(path.to_owned());
    Ok(())
}

pub(crate) fn watch_paths(paths: &[PathBuf], output: &Path) -> io::Result<BTreeSet<PathBuf>> {
    let output = output.canonicalize()?;
    let mut watches = BTreeSet::new();
    let mut links = BTreeSet::new();
    let mut pending: Vec<_> = paths.iter().cloned().map(|path| (path, 0)).collect();
    while let Some((path, depth)) = pending.pop() {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "build watch input must be absolute",
            ));
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if fs::metadata(&path).is_ok_and(|target| target.is_dir()) {
                    // Existing directory links already retain link and target mtimes
                    // in Cargo. Watching their parent would include root build caches.
                    add_watch(&path, &output, &mut watches)?;
                    continue;
                }
                let parent = path.parent().expect("absolute symlink has a parent");
                add_watch(parent, &output, &mut watches)?;
                let link = parent.canonicalize()?.join(path.file_name().unwrap());
                if links.insert(link.clone()) {
                    if depth >= 40 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "build watch exceeds 40 symlink resolutions",
                        ));
                    }
                    let target = fs::read_link(&link)?;
                    pending.push((
                        if target.is_absolute() {
                            target
                        } else {
                            parent.join(target)
                        },
                        depth + 1,
                    ));
                }
            }
            Ok(_) => add_watch(&path, &output, &mut watches)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut parent = path.parent();
                loop {
                    let directory = parent.ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, "no existing build watch ancestor")
                    })?;
                    match fs::symlink_metadata(directory) {
                        Ok(_) => {
                            // A dangling directory link is resolved by the same bounded loop.
                            pending.push((directory.to_owned(), depth));
                            break;
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            parent = directory.parent();
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(watches)
}

fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}
fn main() {
    let directory = env::var_os("CARGO_MANIFEST_DIR").expect("Cargo provides manifest directory");
    let root = Path::new(&directory).join("../../..");
    let commit = git(&root, &["rev-parse", "--verify", "HEAD"]).filter(|value| {
        value.trim().len() == 40 && value.trim().bytes().all(|v| v.is_ascii_hexdigit())
    });
    let commit = commit.as_deref().map(str::trim).unwrap_or("unknown");
    let status = git(
        &root,
        &["status", "--porcelain", "--untracked-files=normal"],
    );
    let tree = match status {
        Some(value) if value.is_empty() => "clean",
        Some(_) => "dirty",
        None => "unknown",
    };
    println!("cargo:rustc-env=OPENPILOT_LOGGING_SOURCE_COMMIT={commit}");
    println!("cargo:rustc-env=OPENPILOT_LOGGING_SOURCE_TREE={tree}");
    let mut watches = vec![
        Path::new(&directory).join("build.rs"),
        Path::new(&directory).join("src"),
    ];
    for name in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", name]) {
            let path = Path::new(path.trim());
            watches.push(if path.is_absolute() {
                path.to_owned()
            } else {
                root.join(path)
            });
        }
    }
    if let Some(reference) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", reference.trim()]) {
            let path = Path::new(path.trim());
            watches.push(if path.is_absolute() {
                path.to_owned()
            } else {
                root.join(path)
            });
        }
    }
    // Track the index and tracked inputs. Missing paths and symlinks also need
    // conservative directory watches, which can include ignored siblings.
    if let Some(files) = git(&root, &["ls-files", "-z"]) {
        for file in files.split('\0').filter(|file| !file.is_empty()) {
            watches.push(root.join(file));
        }
    }
    let output = env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR");
    for path in watch_paths(&watches, Path::new(&output)).expect("resolve existing build watches") {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
