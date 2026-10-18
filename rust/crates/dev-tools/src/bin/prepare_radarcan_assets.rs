fn main() {
    if let Err(error) = openpilot_dev_tools::dbc_assets::radarcan_cli() {
        if error.kind() == "Usage" {
            eprintln!("usage: prepare-radarcan-assets NEW_DIRECTORY");
        } else {
            eprintln!("{error}");
        }
        std::process::exit(1);
    }
}
