#[path = "src/replay/identity.rs"]
#[allow(dead_code)]
mod identity;
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for path in [
        "rust/crates",
        "rust/upstream/dbc-generators.json",
        "rust/upstream/radar-final-source.json",
        "rust/upstream/planner-source.json",
        "rust/upstream/signal-source.json",
        "rust/upstream/signal-runtime.json",
        "rust/Cargo.lock",
        "rust/Cargo.toml",
        "openpilot/cereal",
        "opendbc_repo/opendbc/dbc",
        "opendbc_repo/opendbc/car",
        ".git/HEAD",
        ".git/refs/heads",
        ".git/packed-refs",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    let fingerprint = identity::fingerprint(&root).expect("native replay source fingerprint");
    println!("cargo:rustc-env=RADAR_REPLAY_SOURCE={fingerprint}");
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&root)
        .output()
        .expect("replay source revision");
    assert!(
        revision.status.success(),
        "replay source revision unavailable"
    );
    println!(
        "cargo:rustc-env=RADAR_REPLAY_REVISION={}",
        String::from_utf8(revision.stdout)
            .expect("revision encoding")
            .trim()
    );
}
