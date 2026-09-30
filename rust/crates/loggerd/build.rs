use std::{error::Error, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn Error>> {
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
    if root.join(".git").exists() {
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
                println!(
                    "cargo:rerun-if-changed={}",
                    root.join(String::from_utf8(output.stdout)?.trim())
                        .display()
                );
            }
        }
    }
    println!("cargo:rustc-env=LOGGERD_SOURCE_COMMIT={commit}");
    println!("cargo:rustc-env=LOGGERD_SOURCE_TREE={tree}");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    if let Some(files) = git(&["ls-files", "-z"]) {
        for file in files.split('\0').filter(|file| !file.is_empty()) {
            println!("cargo:rerun-if-changed={}", root.join(file).display());
        }
    }
    let version = std::fs::read_to_string(root.join("openpilot/common/version.h"))?;
    let version = version.split('"').nth(1).ok_or("missing COMMA_VERSION")?;
    println!("cargo:rustc-env=LOGGERD_VERSION={version}");
    Ok(())
}
