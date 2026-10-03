fn main() {
    cxx_build::bridges(["src/scheduling/bridge.rs", "src/params/numeric/bridge.rs"])
        .file("native/scheduling.cc")
        .file("native/params_numeric.cc")
        .include("native")
        .std("c++17")
        .compile("product-ui-scheduling");
    for path in [
        "src/scheduling/bridge.rs",
        "native/scheduling.h",
        "native/scheduling.cc",
        "src/params/numeric/bridge.rs",
        "native/params_numeric.h",
        "native/params_numeric.cc",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
