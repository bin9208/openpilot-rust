use std::{env, path::PathBuf};

fn main() {
    if env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../../..");
    let mut build = cxx_build::bridge("src/bridge.rs");
    build
        .file("native/bridge.cc")
        .file(root.join("msgq_repo/msgq/msgq.cc"))
        .include("native")
        .include(root.join("msgq_repo"))
        .std("c++17")
        .compile("openpilot-msgq-bridge");
    let peer =
        PathBuf::from(env::var_os("OUT_DIR").expect("output directory")).join("native-msgq-peer");
    let mut command = build.get_compiler().to_command();
    command
        .arg("-std=c++17")
        .arg("-I")
        .arg(root.join("msgq_repo"));
    for file in [
        "ipc.cc",
        "event.cc",
        "impl_msgq.cc",
        "impl_fake.cc",
        "msgq.cc",
    ] {
        command.arg(root.join("msgq_repo/msgq").join(file));
    }
    let status = command
        .arg("native/peer.cc")
        .arg("-o")
        .arg(&peer)
        .status()
        .expect("compile native peer");
    assert!(status.success(), "native peer compilation failed");
    println!("cargo:rustc-env=NATIVE_MSGQ_PEER={}", peer.display());
    for path in [
        "src/bridge.rs",
        "native/bridge.h",
        "native/bridge.cc",
        "native/peer.cc",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!(
        "cargo:rerun-if-changed={}",
        root.join("msgq_repo/msgq").display()
    );
}
