use std::{env, fs, io, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=OPENPILOT_OPENCV_ROOT");
    if env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return Ok(());
    }
    let root = PathBuf::from(env::var_os("OPENPILOT_OPENCV_ROOT").ok_or_else(|| {
        io::Error::other("set OPENPILOT_OPENCV_ROOT to the pinned build_xiaoge_opencv.py output")
    })?);
    let manifest = root.join("native-libraries.sha256");
    fs::metadata(&manifest)?;
    let status = Command::new("sha256sum")
        .arg("--check")
        .arg(&manifest)
        .current_dir(&root)
        .status()?;
    if !status.success() {
        return Err(
            io::Error::other("pinned OpenCV library/header SHA verification failed").into(),
        );
    }
    let install = root.join("install");
    cxx_build::bridge("src/bridge.rs")
        .file("native/image.cc")
        .file("native/net.cc")
        .include("native")
        .include(install.join("include/opencv4"))
        .std("c++17")
        .compile("openpilot-opencv-bridge");
    println!(
        "cargo:rustc-link-search=native={}",
        install.join("lib").display()
    );
    for library in ["opencv_dnn", "opencv_imgproc", "opencv_core"] {
        let pinned = fs::canonicalize(install.join("lib").join(format!("lib{library}.so.4.13.0")))?;
        for suffix in ["so", "so.413", "so.4.13.0"] {
            let path = install.join("lib").join(format!("lib{library}.{suffix}"));
            println!("cargo:rerun-if-changed={}", path.display());
            if fs::canonicalize(path)? != pinned {
                return Err(io::Error::other(
                    "OpenCV loader/link alias differs from pinned library",
                )
                .into());
            }
        }
        println!("cargo:rustc-link-lib=dylib={library}");
    }
    for path in [
        "src/bridge.rs",
        "native/bridge.h",
        "native/check.h",
        "native/image.cc",
        "native/net.cc",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!(
        "cargo:rerun-if-changed={}",
        install.join("include/opencv4").display()
    );
    Ok(())
}
