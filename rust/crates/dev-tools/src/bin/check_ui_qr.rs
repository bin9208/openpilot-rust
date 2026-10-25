fn main() {
    if let Err(error) = openpilot_dev_tools::ui_fixed_corpus_check::cli(
        openpilot_dev_tools::ui_fixed_corpus_check::Kind::Qr,
    ) {
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
