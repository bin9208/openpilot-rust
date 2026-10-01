fn main() {
    cxx_build::bridge("src/address.rs")
        .file("native/address.cc")
        .file("native/address_io.cc")
        .include(".")
        .std("c++17")
        .compile("cweb-address");
    for path in [
        "src/address.rs",
        "native/address.cc",
        "native/address.h",
        "native/address_io.cc",
        "native/address_io.h",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
