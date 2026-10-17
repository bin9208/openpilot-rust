use std::{env, path::PathBuf};
#[path = "native/sdk/receipt.rs"]
mod receipt;
fn main() {
    if env::var_os("CARGO_FEATURE_NATIVE_SDK").is_none() {
        return;
    }
    let root = PathBuf::from(
        env::var_os("IMAGE_CODECS_SDK_ROOT")
            .expect("stage the pinned decoder-only image SDK and set IMAGE_CODECS_SDK_ROOT"),
    );
    receipt::verify(&root).expect("decoder SDK receipt must match its installed libraries");
    for library in receipt::LIBRARIES {
        assert!(
            root.join("lib").join(format!("lib{library}.a")).is_file(),
            "missing pinned image codec library {library}"
        );
    }
    cc::Build::new()
        .file("native/decode.c")
        .file("native/tiff.c")
        .include(root.join("include"))
        .include(root.join("include/openjpeg-2.5"))
        .std("c11")
        .warnings(true)
        .compile("openpilot-image-codecs-boundary");
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("lib").display()
    );
    for library in receipt::LIBRARIES {
        println!("cargo:rustc-link-lib=static={library}");
    }
    for library in ["stdc++", "m", "pthread", "dl"] {
        println!("cargo:rustc-link-lib={library}");
    }
    println!("cargo:rerun-if-env-changed=IMAGE_CODECS_SDK_ROOT");
    println!("cargo:rerun-if-changed=native/decode.c");
    println!("cargo:rerun-if-changed=native/decode.h");
    println!("cargo:rerun-if-changed=native/tiff.c");
    println!("cargo:rerun-if-changed=native/tiff.h");
    println!("cargo:rerun-if-changed=native/sdk/receipt.rs");
    println!(
        "cargo:rerun-if-changed={}",
        root.join("openpilot-image-codecs-sdk.txt").display()
    );
    for library in receipt::LIBRARIES {
        println!(
            "cargo:rerun-if-changed={}",
            root.join("lib").join(format!("lib{library}.a")).display()
        );
    }
}
