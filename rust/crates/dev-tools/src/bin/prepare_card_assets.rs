fn main() {
    if let Err(error) = openpilot_dev_tools::dbc_assets::card_cli() {
        if error.kind() == "Usage" {
            eprintln!("usage: prepare-card-assets REPOSITORY NEW_DBC_DIRECTORY");
        } else {
            eprintln!("{error}");
        }
        std::process::exit(1);
    }
}
