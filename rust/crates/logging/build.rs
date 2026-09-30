use std::{env, path::Path, process::Command};
fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
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
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    for name in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", name]) {
            let path = Path::new(path.trim());
            println!(
                "cargo:rerun-if-changed={}",
                if path.is_absolute() {
                    path.to_owned()
                } else {
                    root.join(path)
                }
                .display()
            );
        }
    }
    if let Some(reference) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", reference.trim()]) {
            let path = Path::new(path.trim());
            println!(
                "cargo:rerun-if-changed={}",
                if path.is_absolute() {
                    path.to_owned()
                } else {
                    root.join(path)
                }
                .display()
            );
        }
    }
    // Track source modifications as well as the commit/index; ignored build caches do not participate.
    if let Some(files) = git(&root, &["ls-files", "-z"]) {
        for file in files.split('\0').filter(|file| !file.is_empty()) {
            println!("cargo:rerun-if-changed={}", root.join(file).display());
        }
    }
}
