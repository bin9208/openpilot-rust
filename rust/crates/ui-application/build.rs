fn main() {
    cxx_build::bridges([
        "src/scheduling/bridge.rs",
        "src/params/numeric/bridge.rs",
        "src/agnos_updater/scheduling/bridge.rs",
    ])
    .file("native/scheduling.cc")
    .file("native/params_numeric.cc")
    .file("src/agnos_updater/scheduling/updater_scheduling.cc")
    .include("src/agnos_updater/scheduling")
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
        "src/agnos_updater/scheduling/bridge.rs",
        "src/agnos_updater/scheduling/updater_scheduling.h",
        "src/agnos_updater/scheduling/updater_scheduling.cc",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
