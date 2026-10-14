//! Build inputs must stand alone when the retired source tree is absent.
use std::{fs, path::PathBuf, process::Command, time::SystemTime};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn build(crate_name: &str, outputs: &[&str]) {
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let source = crates.join(crate_name);
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "openpilot-source-free-{crate_name}-{}-{nonce}",
        std::process::id()
    )));
    fs::create_dir(&scratch.0).unwrap();
    let manifest = scratch.0.join("crate");
    let assets = manifest.join("assets");
    let output = scratch.0.join("output");
    fs::create_dir_all(&assets).unwrap();
    fs::create_dir(&output).unwrap();
    for name in outputs {
        let input = source.join("assets").join(name);
        if input.exists() {
            fs::copy(input, assets.join(name)).unwrap();
        }
    }
    let executable = scratch.0.join("build-script");
    let compiled = Command::new("rustc")
        .arg("--edition=2024")
        .arg(source.join("build.rs"))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{crate_name} build script must compile without retired tooling: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let built = Command::new(executable)
        .env_clear()
        .env("PATH", "")
        .env("CARGO_MANIFEST_DIR", &manifest)
        .env("OUT_DIR", &output)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{crate_name} build must work without the original source tree: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    for name in outputs {
        assert!(output.join(name).metadata().unwrap().len() > 0);
        assert!(
            fs::read(output.join(name)).unwrap() == fs::read(assets.join(name)).unwrap(),
            "{crate_name} build changed the canonical asset bytes: {name}"
        );
    }
}

#[test]
fn server_build_without_original_source() {
    build("carrot-server", &["vision_diag_defaults.rs"]);
}

#[test]
fn dashcam_build_without_original_source() {
    build("dashcam-upload", &["source_defaults.rs"]);
}

#[test]
fn recovery_build_without_original_source() {
    build(
        "carrot-recovery",
        &["index.html", "recovery.js", "recovery-api.js"],
    );
}

#[test]
fn gps_build_without_original_source() {
    build("qcomgpsd", &["reports.rs"]);
}
