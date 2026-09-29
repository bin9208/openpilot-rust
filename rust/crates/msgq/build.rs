use std::{env, path::PathBuf};

fn main() {
    if env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../../..");
    let ion = env::var_os("CARGO_FEATURE_VISIONIPC_ION").is_some();
    let mut build = cxx_build::bridges(["src/bridge.rs", "src/vision_bridge.rs"]);
    build
        .file("native/bridge.cc")
        .file("native/vision.cc")
        .file(root.join("msgq_repo/msgq/msgq.cc"))
        .include("native")
        .include(root.join("msgq_repo"))
        .std("c++17")
        .flag("-UNDEBUG");
    let sources = [
        "ipc.cc",
        "event.cc",
        "impl_msgq.cc",
        "impl_fake.cc",
        "visionipc/visionipc.cc",
        "visionipc/visionipc_client.cc",
        "visionipc/visionipc_server.cc",
        if ion {
            "visionipc/visionbuf_ion.cc"
        } else {
            "visionipc/visionbuf.cc"
        },
    ];
    for source in sources {
        build.file(root.join("msgq_repo/msgq").join(source));
    }
    if ion {
        build.include(root.join("third_party/linux/include"));
    }
    build.compile("openpilot-msgq-bridge");
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
    let vision_peer =
        PathBuf::from(env::var_os("OUT_DIR").expect("output directory")).join("native-vision-peer");
    let mut command = build.get_compiler().to_command();
    command
        .arg("-std=c++17")
        .arg("-pthread")
        .arg("-I")
        .arg(root.join("msgq_repo"));
    if ion {
        command
            .arg("-I")
            .arg(root.join("third_party/linux/include"));
    }
    for source in sources.into_iter().chain(["msgq.cc"]) {
        command.arg(root.join("msgq_repo/msgq").join(source));
    }
    let status = command
        .arg("native/vision_peer.cc")
        .arg("-o")
        .arg(&vision_peer)
        .status()
        .expect("compile native VisionIPC peer");
    assert!(status.success(), "native VisionIPC peer compilation failed");
    println!(
        "cargo:rustc-env=NATIVE_VISION_PEER={}",
        vision_peer.display()
    );
    for path in [
        "src/bridge.rs",
        "src/vision_bridge.rs",
        "native/bridge.h",
        "native/bridge.cc",
        "native/peer.cc",
        "native/vision.h",
        "native/vision.cc",
        "native/vision_peer.cc",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!(
        "cargo:rerun-if-changed={}",
        root.join("msgq_repo/msgq").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("third_party/linux/include").display()
    );
}
