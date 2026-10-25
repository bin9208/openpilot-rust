fn main() {
    if let Some(result) = openpilot_dev_tools::bootlog_artifact_check::command_fixture() {
        match result {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if let Err(error) = openpilot_dev_tools::bootlog_artifact_check::cli() {
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
