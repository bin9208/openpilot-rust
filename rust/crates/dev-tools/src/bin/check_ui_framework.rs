fn main() {
    match openpilot_dev_tools::ui_framework_check::cli() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
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
}
