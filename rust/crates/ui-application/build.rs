fn main() {
    cxx_build::bridge("src/scheduling/bridge.rs")
        .file("native/scheduling.cc")
        .include("native")
        .std("c++17")
        .compile("product-ui-scheduling");
    for path in [
        "src/scheduling/bridge.rs",
        "native/scheduling.h",
        "native/scheduling.cc",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
