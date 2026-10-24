fn main() {
    match openpilot_dev_tools::journald_fixture::cli() {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
