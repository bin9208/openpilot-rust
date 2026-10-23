fn main() {
    match openpilot_dev_tools::developer_ssh::ssh_cli() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
