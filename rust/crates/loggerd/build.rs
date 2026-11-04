use std::{
    error::Error,
    path::{Path, PathBuf},
    process::Command,
};

fn watch_git_path(path: &Path, refs: Option<&Path>) {
    if path.is_file() {
        println!("cargo:rerun-if-changed={}", path.display());
    } else if let Some(refs) = refs {
        // A packed branch can acquire a loose ref without changing packed-refs.
        // Stay within refs; watching all of .git would also track objects/logs.
        for parent in path.ancestors().skip(1).take_while(|p| p.starts_with(refs)) {
            if parent.is_dir() {
                println!("cargo:rerun-if-changed={}", parent.display());
                break;
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=OPENPILOT_NATIVE_INCREMENTAL");
    let incremental = match std::env::var("OPENPILOT_NATIVE_INCREMENTAL").as_deref() {
        Err(std::env::VarError::NotPresent) => false,
        Ok("1") => true,
        _ => return Err("OPENPILOT_NATIVE_INCREMENTAL must be absent or exactly 1".into()),
    };
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("../../..");
    println!(
        "cargo:rerun-if-changed={}",
        root.join("openpilot/common/version.h").display()
    );
    let git = |arguments: &[&str]| {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&root)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).ok())
            .flatten()
    };
    let commit = git(&["rev-parse", "--verify", "HEAD"]).filter(|text| {
        text.trim().len() == 40 && text.trim().bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    let commit = commit.as_deref().map(str::trim).unwrap_or("unknown");
    let tree = match git(&["status", "--porcelain", "--untracked-files=normal"]) {
        Some(text) if text.is_empty() => "clean",
        Some(_) => "dirty",
        None => "unknown",
    };
    if !incremental && root.join(".git").exists() {
        let reference = Command::new("git")
            .args(["symbolic-ref", "--quiet", "HEAD"])
            .current_dir(&root)
            .output()?;
        let reference = String::from_utf8(reference.stdout)?;
        for reference in ["HEAD", "index", "packed-refs", reference.trim()]
            .into_iter()
            .filter(|reference| !reference.is_empty())
        {
            let output = Command::new("git")
                .args(["rev-parse", "--git-path", reference])
                .current_dir(&root)
                .output()?;
            if output.status.success() {
                let path = root.join(String::from_utf8(output.stdout)?.trim());
                let refs = if reference.starts_with("refs/") {
                    git(&["rev-parse", "--git-path", "refs"]).map(|path| root.join(path.trim()))
                } else {
                    None
                };
                watch_git_path(&path, refs.as_deref());
            }
        }
    }
    println!("cargo:rustc-env=LOGGERD_SOURCE_COMMIT={commit}");
    println!("cargo:rustc-env=LOGGERD_SOURCE_TREE={tree}");
    println!(
        "cargo:rustc-env=LOGGERD_SOURCE_STAMP_MODE={}",
        if incremental {
            "compiled-native-incremental"
        } else {
            "exact-source"
        }
    );
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if let Some(files) = (!incremental).then(|| git(&["ls-files", "-z"])).flatten() {
        for file in files.split('\0').filter(|file| !file.is_empty()) {
            let path = root.join(file);
            // Cargo follows the path when reading timestamps. A tracked
            // dangling symlink is permanently "missing" and otherwise forces
            // every dependent binary to rebuild. Watch its existing parent
            // for link replacement/creation, retaining Git index/HEAD watches.
            let watched = if path.exists() {
                path.as_path()
            } else {
                path.ancestors()
                    .skip(1)
                    .find(|parent| parent.is_dir())
                    .expect("tracked input has an existing source parent")
            };
            println!("cargo:rerun-if-changed={}", watched.display());
        }
    }
    let version = std::fs::read_to_string(root.join("openpilot/common/version.h"))?;
    let version = version.split('"').nth(1).ok_or("missing COMMA_VERSION")?;
    println!("cargo:rustc-env=LOGGERD_VERSION={version}");
    Ok(())
}
