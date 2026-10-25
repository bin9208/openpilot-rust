fn main() {
    if let Err(error) = openpilot_dev_tools::carrot_navi_compare::cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
