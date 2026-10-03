use std::{env, path::PathBuf, process::Command};
fn checked(command: &mut Command) {
    let status = command.status().expect("start native JPEG build command");
    assert!(
        status.success(),
        "native JPEG build command failed: {command:?}"
    );
}
fn main() {
    if env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return;
    }
    let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("native/vendor");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output")).join("jpeg");
    let compiler = cc::Build::new().get_compiler();
    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(&source)
        .arg("-B")
        .arg(&output)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DCMAKE_POSITION_INDEPENDENT_CODE=ON")
        .arg("-DENABLE_SHARED=OFF")
        .arg("-DENABLE_STATIC=ON")
        .arg("-DWITH_TURBOJPEG=OFF")
        .arg("-DWITH_TOOLS=OFF")
        .arg("-DWITH_TESTS=OFF")
        .arg("-DWITH_SIMD=OFF")
        .arg(format!("-DCMAKE_C_COMPILER={}", compiler.path().display()));
    if env::var("HOST").ok() != env::var("TARGET").ok() {
        configure.arg("-DCMAKE_SYSTEM_NAME=Linux").arg(format!(
            "-DCMAKE_SYSTEM_PROCESSOR={}",
            env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture")
        ));
    }
    checked(&mut configure);
    checked(
        Command::new("cmake")
            .arg("--build")
            .arg(&output)
            .arg("--target")
            .arg("jpeg-static")
            .arg("--parallel")
            .arg("2"),
    );
    cxx_build::bridge("src/bridge.rs")
        .file("native/bridge.cc")
        .include("native")
        .std("c++17")
        .compile("openpilot-jpeg-bridge");
    cc::Build::new()
        .file("native/encode.c")
        .include(source.join("src"))
        .include(&output)
        .std("c11")
        .compile("openpilot-jpeg-encode");
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=jpeg");
    println!("cargo:source_include={}", source.join("src").display());
    println!("cargo:build_include={}", output.display());
    for path in [
        "src/bridge.rs",
        "native/bridge.h",
        "native/bridge.cc",
        "native/encode.h",
        "native/encode.c",
        "native/vendor",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
