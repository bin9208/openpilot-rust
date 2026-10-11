use std::{env, fs, io, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=OPENPILOT_WEBCAM_OPENCV_ROOT");
    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return Ok(());
    }
    let root = PathBuf::from(env::var_os("OPENPILOT_WEBCAM_OPENCV_ROOT").ok_or_else(|| {
        io::Error::other("set OPENPILOT_WEBCAM_OPENCV_ROOT to build_webcam_opencv.py output")
    })?);
    let manifest = root.join("native-libraries.sha256");
    fs::metadata(&manifest)?;
    if !Command::new("sha256sum")
        .arg("--check")
        .arg(&manifest)
        .current_dir(&root)
        .status()?
        .success()
    {
        return Err(io::Error::other("webcam OpenCV provider SHA verification failed").into());
    }
    let install = root.join("install");
    cxx_build::bridge("src/capture/bridge.rs")
        .file("native/capture.cc")
        .include("native")
        .include(install.join("include/opencv4"))
        .std("c++17")
        .compile("openpilot-webcam-capture");
    println!(
        "cargo:rustc-link-search=native={}",
        install.join("lib").display()
    );
    for library in ["videoio", "imgcodecs", "imgproc", "core"] {
        let stem = format!("libopencv_{library}");
        let pinned = fs::canonicalize(install.join("lib").join(format!("{stem}.so.4.13.0")))?;
        for suffix in ["so", "so.413", "so.4.13.0"] {
            let path = install.join("lib").join(format!("{stem}.{suffix}"));
            if fs::canonicalize(&path)? != pinned {
                return Err(
                    io::Error::other("OpenCV link alias differs from pinned library").into(),
                );
            }
            println!("cargo:rerun-if-changed={}", path.display());
        }
        println!("cargo:rustc-link-lib=dylib=opencv_{library}");
    }
    for path in [
        "src/capture/bridge.rs",
        "native/capture.h",
        "native/capture.cc",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed={}", manifest.display());
    Ok(())
}
