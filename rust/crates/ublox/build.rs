fn main() {
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    cxx_build::bridge("src/bridge.rs")
        .file("native/serial.cc")
        .include("native")
        .std("c++17")
        .compile("ublox-modem-lines");
    for path in ["src/bridge.rs", "native/serial.cc", "native/serial.h"] {
        println!("cargo:rerun-if-changed={path}");
    }
}
