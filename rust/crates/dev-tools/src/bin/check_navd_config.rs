#[cfg(feature = "navd-config-tools-native")]
fn main() {
    let result = openpilot_dev_tools::navd_config_check::startup_phase("00-main-entry")
        .and_then(|()| openpilot_dev_tools::navd_config_check::cli());
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(
            if error
                .downcast_ref::<openpilot_dev_tools::argparse_scan::ArgumentError>()
                .is_some()
            {
                2
            } else {
                1
            },
        );
    }
}

#[cfg(not(feature = "navd-config-tools-native"))]
fn main() {
    eprintln!("check-navd-config requires navd-config-tools-native");
    std::process::exit(2);
}
