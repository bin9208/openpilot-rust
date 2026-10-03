use std::{env, path::PathBuf};

fn main() {
    if env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../../..");
    let ion = env::var_os("CARGO_FEATURE_VISIONIPC_ION").is_some();
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .cargo_metadata(false)
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
    let abi_peer = PathBuf::from(env::var_os("OUT_DIR").expect("output directory"))
        .join("native-ipc-abi-peer");
    let status = build
        .get_compiler()
        .to_command()
        .args(["-std=c++17", "-I"])
        .arg(root.join("msgq_repo"))
        .arg("-I")
        .arg(root.join("third_party/linux/include"))
        .arg("native/abi_peer.cc")
        .arg("-o")
        .arg(&abi_peer)
        .status()
        .expect("compile native IPC ABI peer");
    assert!(status.success(), "native IPC ABI peer compilation failed");
    println!("cargo:rustc-env=NATIVE_IPC_ABI_PEER={}", abi_peer.display());
    for path in [
        "native/peer.cc",
        "native/vision_peer.cc",
        "native/abi_peer.cc",
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
