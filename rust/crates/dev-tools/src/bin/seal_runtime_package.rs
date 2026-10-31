use openpilot_dev_tools::seal::{seal, Result, BOOT_BINARIES};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn run() -> Result<()> {
    let mut root = None;
    let mut manager = None;
    let mut architecture = None;
    let mut source_build = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().ok_or("argument requires a value")?;
        match arg.to_str(){Some("--root")=>root=Some(PathBuf::from(value)),Some("--manager")=>manager=Some(PathBuf::from(value)),Some("--architecture")=>architecture=Some(value.into_string().map_err(|_|"invalid architecture")?),Some("--source-build") if source_build.is_none()=>source_build=Some(PathBuf::from(value)),_=>return Err("usage: seal-runtime-package --root ROOT --manager HOST_MANAGER --architecture aarch64|x86_64 [--source-build VERIFIED_SOURCE_JSON]".into())}
    }
    let root = root.ok_or("--root required")?.canonicalize()?;
    let manager = manager.ok_or("--manager required")?.canonicalize()?;
    let architecture = architecture.ok_or("--architecture required")?;
    let commit = if let Some(path) = source_build {
        openpilot_runtime_build::generation::SourceForSeal::read_and_verify(&path, &root)?
            .source
            .source
            .commit
    } else {
        if !git(
            &root,
            &["status", "--porcelain", "--untracked-files=normal"],
        )?
        .is_empty()
        {
            return Err("source checkout must be clean before sealing a runtime candidate".into());
        }
        git(&root, &["rev-parse", "HEAD"])?
    };
    let output = Command::new(manager)
        .arg("--root")
        .arg(&root)
        .arg("--inspect")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "manager inventory failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let inventory = serde_json::from_slice(&output.stdout)?;
    let manifest = seal(
        &root,
        &inventory,
        &architecture,
        &commit,
        &BOOT_BINARIES,
        &["usbgpu-assets"],
    )?;
    println!(
        "{}",
        serde_json::json!({"source_commit":commit,"architecture":architecture,"artifacts":manifest["files"].as_array().ok_or("invalid manifest")?.len()})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("seal-runtime-package: {error}");
        std::process::exit(1);
    }
}
