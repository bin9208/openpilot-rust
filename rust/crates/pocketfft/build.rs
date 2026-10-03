fn main() {
    if std::env::var_os("CARGO_FEATURE_NATIVE_SKIP_MIRI").is_none() {
        return;
    }
    cxx_build::bridge("src/bridge.rs")
        .file("native/bridge.cc")
        .include("native")
        .std("c++17")
        .flag("-ffp-contract=off")
        .compile("openpilot-pocketfft");
    for path in [
        "src/lib.rs",
        "src/bridge.rs",
        "native/bridge.cc",
        "native/bridge.h",
        "native/vendor/pocketfft_hdronly.h",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
