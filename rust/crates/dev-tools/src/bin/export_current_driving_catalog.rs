#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = openpilot_dev_tools::current_catalog_export::cli() {
        eprintln!("{error}");
        std::process::exit(
            if error.is::<openpilot_dev_tools::argparse_scan::ArgumentError>() {
                2
            } else {
                1
            },
        );
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("export-current-driving-catalog requires Linux");
    std::process::exit(1);
}
