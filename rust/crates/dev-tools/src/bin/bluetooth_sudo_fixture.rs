fn main() {
    match openpilot_dev_tools::bluetooth_sudo_fixture::cli() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
