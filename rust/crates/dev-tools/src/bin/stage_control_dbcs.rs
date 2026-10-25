fn main() {
    if let Err(error) = openpilot_dev_tools::dbc_generator::stage_cli() {
        eprintln!("{error}");
        std::process::exit(if error.kind() == "ArgumentError" {
            2
        } else {
            1
        });
    }
}
