fn main() {
    if std::env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return;
    }
    let mut build = cxx_build::bridge("src/usb_bridge.rs");
    if std::env::var_os("USBGPU_SANITIZE").is_some() {
        build
            .flag("-fsanitize=address,undefined")
            .flag("-fno-omit-frame-pointer");
        println!("cargo:rustc-link-lib=asan");
        println!("cargo:rustc-link-lib=ubsan");
    }
    println!("cargo:rerun-if-env-changed=USBGPU_SANITIZE");
    build
        .file("native/usb.cc")
        .file("native/transfers.cc")
        .include("native")
        .std("c++17")
        .compile("openpilot-usbgpu-usb");
    println!("cargo:rustc-link-lib=dl");
    for path in [
        "src/usb_bridge.rs",
        "native/usb.h",
        "native/usb_api.h",
        "native/usb.cc",
        "native/transfers.cc",
        "native/vendor/libusb.h",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
