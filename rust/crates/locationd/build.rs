use std::{env, path::PathBuf};

fn main() {
    if env::var_os("CARGO_FEATURE_SOLVER").is_none() {
        return;
    }
    let rednose = PathBuf::from("../../../rednose_repo");
    let mut build = cxx_build::bridge("src/bridge.rs");
    build
        .file("native/filter.cc")
        .file("native/model.cc")
        .file("native/scheduler.cc")
        .file(rednose.join("rednose/helpers/ekf_sym.cc"))
        .file(rednose.join("rednose/helpers/ekf_load.cc"))
        .include("native")
        .include(&rednose)
        .include(rednose.join("rednose"))
        .std("c++17");
    if let Some(include) = env::var_os("LOCATIOND_EIGEN_INCLUDE") {
        build.include(include);
    }
    if let Some(libraries) = env::var_os("LOCATIOND_SANITIZER_LIBS") {
        build
            .flag("-fsanitize=address,undefined")
            .flag("-fno-omit-frame-pointer");
        for library in env::split_paths(&libraries) {
            println!("cargo:rustc-link-arg={}", library.display());
        }
    }
    build.compile("locationd-rednose");
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rerun-if-env-changed=LOCATIOND_EIGEN_INCLUDE");
    println!("cargo:rerun-if-env-changed=LOCATIOND_SANITIZER_LIBS");
    for path in [
        "src/bridge.rs",
        "native",
        "../../../rednose_repo/rednose/helpers",
        "../../../rednose_repo/rednose/templates/ekf_c.c",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
