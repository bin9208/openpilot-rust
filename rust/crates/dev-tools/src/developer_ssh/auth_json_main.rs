fn main() {
    match openpilot_dev_tools::developer_ssh::login::auth_json_cli() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
