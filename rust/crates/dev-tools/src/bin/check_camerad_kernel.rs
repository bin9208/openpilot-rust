fn main() {
    match openpilot_dev_tools::camera_kernel_check::cli() {
        Ok(failures) => std::process::exit(i32::from(failures != 0)),
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
