#[path = "../encoder_runtime_native.rs"]
mod native;
fn main() {
    if let Err(error) = native::cli() {
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
